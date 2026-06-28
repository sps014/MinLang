use std::collections::HashMap;
use std::io::Error;
use std::rc::Rc;
use std::cell::RefCell;
use crate::syntax::nodes::{FunctionNode, Type};
use crate::syntax::nodes::types::{canonical_type_name, mangle_generic, mangle_with_suffixes, method_fn, release_func_suffix, strip_nullable, value_size_align};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use crate::semantics::symbol_table::SymbolTable;
use crate::semantics::function_table::{OverloadResolution, overload_arg_compatible};
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Resolves a (possibly generic) function call to its concrete, mangled name.
    /// Uses explicit generic arguments when present, otherwise infers the type from the
    /// first argument and falls back to the plain name when no monomorphized variant exists.
    pub fn resolve_call_name(&self, name: &str, generic_args: &Option<Vec<Type>>, args: &[crate::syntax::nodes::ExpressionNode<'a>], function: &FunctionNode<'a>) -> String {
        if let Some(generics) = generic_args {
            if !generics.is_empty() {
                return mangle_generic(name, generics);
            }
        }
        // Overloaded free functions: pick the emitted variant whose signature matches the argument
        // types, mirroring the analyzer's selection so both agree on the callee.
        if self.function_table.is_overloaded(name) {
            let arg_types: Vec<String> = args.iter()
                .map(|arg| self.infer_expression_type(arg, function).unwrap_or_default())
                .collect();
            let compat = |param: &str, arg: &str| overload_arg_compatible(param, arg, |t| self.enums.contains_key(t));
            if let OverloadResolution::Unique(key) = self.function_table.select_overload(name, &arg_types, compat) {
                return key;
            }
        }
        name.to_string()
    }

    /// Resolves a method call `obj.method(params)` to the emitted function name, selecting among
    /// overloads by argument types (the receiver is supplied as the implicit `this` argument).
    /// Returns the bare `{struct}_{method}` base when the method is not overloaded.
    pub fn resolve_method_key(&self, struct_name: &str, method: &str, params: &[crate::syntax::nodes::ExpressionNode<'a>], function: &FunctionNode<'a>) -> String {
        let base = method_fn(struct_name, method);
        if !self.function_table.is_overloaded(&base) {
            return base;
        }
        let mut arg_types = Vec::with_capacity(params.len() + 1);
        arg_types.push(struct_name.to_string());
        for param in params {
            arg_types.push(self.infer_expression_type(param, function).unwrap_or_default());
        }
        let compat = |p: &str, a: &str| overload_arg_compatible(p, a, |t| self.enums.contains_key(t));
        match self.function_table.select_overload(&base, &arg_types, compat) {
            OverloadResolution::Unique(key) => key,
            _ => base,
        }
    }

    /// True if `name` is a local variable/parameter of the function currently being emitted.
    pub fn is_local_var(&self, name: &str, function: &FunctionNode<'a>) -> bool {
        let func_name = self.ctx.current_mangled_name.as_ref().unwrap_or(&function.name.text);
        self.ctx.combined_symbol_lookup.get(func_name)
            .map(|m| m.contains_key(name))
            .unwrap_or(false)
    }

    /// If `obj.method(...)` is actually a static call `Type.method(...)` (the receiver names a type,
    /// not a local value, and `{type}_{method}` exists), returns the emitted function name to call
    /// (overload-resolved over the explicit arguments, which carry no implicit `this`).
    pub fn resolve_static_call(&self, obj: &crate::syntax::nodes::ExpressionNode<'a>, method: &str, params: &[crate::syntax::nodes::ExpressionNode<'a>], function: &FunctionNode<'a>) -> Option<String> {
        let crate::syntax::nodes::ExpressionNode::Identifier(id) = obj else { return None; };
        if id.text == "Math" || self.is_local_var(&id.text, function) {
            return None;
        }
        let type_name = canonical_type_name(&id.text).unwrap_or(id.text.as_str()).to_string();
        let base = method_fn(&type_name, method);
        if !(self.function_table.is_overloaded(&base) || self.function_table.get_function(&base).is_ok()) {
            return None;
        }
        Some(self.resolve_call_name(&base, &None, params, function))
    }

    /// The monomorphized struct name a constructor call `Name(...)` / `Name<T>(...)` targets,
    /// mirroring the mangling used by struct instantiation (e.g. `Point<int>` -> `Point_int`).
    pub fn constructor_struct_name(&self, name: &str, generic_args: &Option<Vec<Type>>) -> String {
        match generic_args {
            Some(args) if !args.is_empty() => {
                let mut mangled = name.to_string();
                for arg in args {
                    mangled.push('_');
                    mangled.push_str(&self.resolve_type(&arg.get_type()));
                }
                mangled
            }
            _ => name.to_string(),
        }
    }

    /// Returns true if `expr` produces an *owned* reference: a freshly created value (or a call
    /// result that the callee returned with +1) that already carries the single reference its
    /// consumer takes over. Owned values must NOT be retained again when bound/stored/returned,
    /// otherwise their refcount never reaches 0 and `drop` never runs; conversely, when used in a
    /// borrowing position (call argument) they must be released afterwards.
    ///
    /// Borrowed expressions (identifiers, field/element reads, `this`, string literals, `null`)
    /// reference a value someone else owns, so consumers must retain them. Ambiguous producers
    /// (`Ternary`, `??`, explicit `Cast`) are treated as borrowed — the conservative choice that
    /// at worst leaks, never double-frees.
    pub fn produces_owned_ref(&self, expr: &crate::syntax::nodes::ExpressionNode<'a>, function: &FunctionNode<'a>) -> bool {
        use crate::syntax::nodes::ExpressionNode;
        match expr {
            ExpressionNode::StructInstantiation(_, _, _) | ExpressionNode::ArrayLiteral(_) => true,
            // User struct methods hand back an owned +1 (via `build_return`). `EnumValue.name()`
            // is the exception: it returns an *interned* (borrowed) string that must never be
            // released, so it is treated as borrowed.
            ExpressionNode::MethodCall(_, method, _, _) => method.text != "name",
            // String concatenation allocates a fresh string; comparison/logical operators yield
            // non-reference values (irrelevant to ownership).
            ExpressionNode::Binary(_, _, _) => true,
            ExpressionNode::FunctionCall(n, generic_args, args) => {
                // `array_new<T>(n)` allocates a fresh array.
                if n.text == "array_new" {
                    return true;
                }
                // `print`/`println`/`hash_code` never yield an owned reference. `to_string` may
                // pass through a borrowed string (string argument), so treat it conservatively as
                // borrowed (at worst leaks the freshly formatted string, never over-releases).
                if matches!(n.text.as_str(), "print" | "println" | "hash_code" | "to_string") {
                    return false;
                }
                // Indirect call through a function-typed local: the callee returns +1.
                if self.function_typed_local(&n.text, function).is_some() {
                    return true;
                }
                // Direct function call (callee returns +1) or constructor call (fresh allocation).
                let function_name = self.resolve_call_name(&n.text, generic_args, args, function);
                if self.function_table.get_function(&function_name).is_ok() {
                    return true;
                }
                let ctor_name = self.constructor_struct_name(&n.text, generic_args);
                self.struct_table.get_struct(&ctor_name).is_some()
            }
            ExpressionNode::Parenthesized(inner) => self.produces_owned_ref(inner, function),
            // Identifier, MemberAccess, IndexAccess, Literal, Ternary, `??`, Cast, Unary, Is: borrowed.
            _ => false,
        }
    }

    /// True when `expr` flowing into a slot of type `target_type` is implicitly boxed by
    /// `build_expression` (a primitive value flowing into an `object` slot), producing a fresh
    /// (owned) heap object.
    pub fn will_box(&self, expr: &crate::syntax::nodes::ExpressionNode<'a>, target_type: &str, function: &FunctionNode<'a>) -> Result<bool, Error> {
        if strip_nullable(target_type) != "object" {
            return Ok(false);
        }
        let arg_ty = self.infer_expression_type(expr, function)?;
        Ok(WasmGenerator::is_primitive_name(strip_nullable(&arg_ty)))
    }

    /// Whether the value of `expr`, when stored into a slot of type `target_type`, is an *owned*
    /// reference the slot takes ownership of (so it must not be retained again). Combines the
    /// expression classifier with implicit boxing into an `object` slot.
    pub fn stores_owned_ref(&self, expr: &crate::syntax::nodes::ExpressionNode<'a>, target_type: &str, function: &FunctionNode<'a>) -> Result<bool, Error> {
        Ok(self.will_box(expr, target_type, function)? || self.produces_owned_ref(expr, function))
    }

    /// Builds one call argument and, when it yields an *owned* reference (including a primitive
    /// implicitly boxed into an `object` parameter), `local.tee`s it into a fresh `$tmp{n}` and
    /// records `(slot, release_type)` so [`release_call_temps`] can release it after the call.
    /// The value is left on the operand stack as the argument either way.
    pub fn build_call_arg(&mut self, expr: &crate::syntax::nodes::ExpressionNode<'a>, param_type: &str, function: &FunctionNode<'a>, owned: &mut Vec<(usize, String)>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let pt_base = strip_nullable(param_type).to_string();
        let will_box = self.will_box(expr, param_type, function)?;
        let owns_ref = will_box || self.produces_owned_ref(expr, function);
        self.build_expression(expr, &param_type.to_string(), function, writer)?;
        if self.is_reference_type(&pt_base) && owns_ref {
            let slot = self.ctx.tmp_depth.min(Self::TMP_POOL - 1);
            self.ctx.tmp_depth += 1;
            writer.write_line(&format!("local.tee $tmp{}", slot));
            // A boxed primitive is released as an `object`; otherwise use the parameter's type.
            let release_type = if will_box { "object".to_string() } else { pt_base };
            owned.push((slot, release_type));
        }
        Ok(())
    }

    /// Releases the owned-argument temporaries recorded by [`build_call_arg`] (LIFO) and restores
    /// `tmp_depth` to `saved_depth`. Each release is stack-neutral, so a value left on the stack
    /// by the preceding `call` (the call's result) is preserved.
    pub fn release_call_temps(&mut self, owned: &[(usize, String)], saved_depth: usize, writer: &mut IndentedTextWriter) {
        for (slot, release_type) in owned.iter().rev() {
            writer.write_line(&format!("local.get $tmp{}", slot));
            self.emit_release(release_type, writer);
        }
        self.ctx.tmp_depth = saved_depth;
    }

    /// Returns true if the method invoked as `obj.method(...)` yields a non-void value
    /// (used to decide whether a statement-level invocation must `drop` the result).
    pub fn method_returns_value(&self, obj: &crate::syntax::nodes::ExpressionNode<'a>, method: &crate::syntax::token::syntax_token::SyntaxToken, params: &[crate::syntax::nodes::ExpressionNode<'a>], function: &FunctionNode<'a>) -> Result<bool, Error> {
        // `Math.<fn>(...)` always yields a float.
        if let crate::syntax::nodes::ExpressionNode::Identifier(id) = obj {
            if id.text == "Math" {
                return Ok(true);
            }
        }
        if let Some(key) = self.resolve_static_call(obj, &method.text, params, function) {
            let returns_value = self.function_table.get_function(&key)
                .ok()
                .and_then(|info| info.return_type)
                .map(|ret| ret.get_type() != "void")
                .unwrap_or(false);
            return Ok(returns_value);
        }
        let obj_type = self.infer_expression_type(obj, function)?;
        let struct_name = strip_nullable(&obj_type).to_string();
        if method.text == "len" && (struct_name.ends_with("[]") || struct_name == "string") {
            return Ok(true);
        }
        let mangled_name = self.resolve_method_key(&struct_name, &method.text, params, function);
        let returns_value = self.function_table.get_function(&mangled_name)
            .ok()
            .and_then(|info| info.return_type)
            .map(|ret| ret.get_type() != "void")
            .unwrap_or(false);
        Ok(returns_value)
    }

    /// The return type name of `obj.method(...)`, or `None` for `void`/unknown. Used to decide
    /// whether a discarded statement-level method result should be released (owned reference),
    /// dropped (non-reference value), or ignored (void).
    pub fn method_return_type(&self, obj: &crate::syntax::nodes::ExpressionNode<'a>, method: &crate::syntax::token::syntax_token::SyntaxToken, params: &[crate::syntax::nodes::ExpressionNode<'a>], function: &FunctionNode<'a>) -> Result<Option<String>, Error> {
        if let crate::syntax::nodes::ExpressionNode::Identifier(id) = obj {
            if id.text == "Math" {
                return Ok(Some("float".to_string()));
            }
        }
        if let Some(key) = self.resolve_static_call(obj, &method.text, params, function) {
            return Ok(self.function_table.get_function(&key)
                .ok()
                .and_then(|info| info.return_type)
                .map(|ret| ret.get_type()));
        }
        let obj_type = self.infer_expression_type(obj, function)?;
        let struct_name = strip_nullable(&obj_type).to_string();
        if method.text == "len" && (struct_name.ends_with("[]") || struct_name == "string") {
            return Ok(Some("int".to_string()));
        }
        if method.text == "name" && self.enums.contains_key(&struct_name) {
            return Ok(Some("string".to_string()));
        }
        let mangled_name = self.resolve_method_key(&struct_name, &method.text, params, function);
        Ok(self.function_table.get_function(&mangled_name)
            .ok()
            .and_then(|info| info.return_type)
            .map(|ret| ret.get_type()))
    }

    /// The byte size of a single element of the given (non-pointer) type.
    /// Pointers (arrays, structs, strings) and `int`/`float` are 4 bytes.
    pub fn element_size_of(type_name: &str) -> usize {
        value_size_align(type_name).0
    }

    /// Emits a store instruction appropriate for a value of `type_name` already on the stack
    /// (address and value must already be pushed).
    pub fn emit_store(type_name: &str, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let instruction = match WasmGenerator::get_wasm_type_from(type_name.to_string())?.as_str() {
            _ if type_name == "bool" || type_name == "char" => "i32.store8",
            "f64" => "f64.store",
            "f32" => "f32.store",
            _ => "i32.store",
        };
        writer.write_line(instruction);
        Ok(())
    }

    /// Emits a load instruction appropriate for a value of `type_name`
    /// (the address must already be on the stack).
    pub fn emit_load(type_name: &str, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let instruction = match WasmGenerator::get_wasm_type_from(type_name.to_string())?.as_str() {
            _ if type_name == "bool" || type_name == "char" => "i32.load8_u",
            "f64" => "f64.load",
            "f32" => "f32.load",
            _ => "i32.load",
        };
        writer.write_line(instruction);
        Ok(())
    }

    /// Emits a `$release_*` call for the given (possibly nullable/array) reference type.
    pub fn emit_release(&self, type_name: &str, writer: &mut IndentedTextWriter) {
        writer.write_line(&format!("call $release_{}", release_func_suffix(strip_nullable(type_name))));
    }

    /// Retains every reference-typed parameter on function entry so the matching releases at
    /// every exit point keep reference counts balanced (the callee owns its parameter bindings).
    pub fn emit_retain_params(&self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) {
        for param in &function.parameters {
            let base = strip_nullable(&self.resolve_type(&param.type_.get_type())).to_string();
            if self.is_reference_type(&base) {
                writer.write_line(&format!("local.get ${}", param.name.text));
                writer.write_line("call $retain");
            }
        }
    }

    /// Releases every reference-typed local (and parameter) recorded for `func_name`.
    /// Used both on fall-through exit and before an explicit `return`.
    pub fn emit_release_locals(&self, func_name: &str, writer: &mut IndentedTextWriter) {
        let locals = self.ctx.combined_symbol_lookup.get(func_name).unwrap().clone();
        for (name, type_) in locals.iter() {
            let base = strip_nullable(&type_.get_type()).to_string();
            if self.is_reference_type(&base) {
                writer.write_line(&format!("local.get ${}", name));
                self.emit_release(&base, writer);
            }
        }
    }
    /// Gets the WebAssembly type string from a Dream type name
    pub fn get_wasm_type_from(typename: String) -> Result<String, Error> {
        let base_type = if typename.ends_with("[]") {
            // Arrays are represented as pointers (i32)
            return Ok("i32".to_string());
        } else {
            typename.as_str()
        };

        let r = match base_type {
            "int" => "i32".to_string(),
            "float" => "f32".to_string(),
            "double" => "f64".to_string(),
            "bool" => "i32".to_string(),
            "char" => "i32".to_string(),
            "string" => "i32".to_string(),
            "void" => "".to_string(),
            _ => {
                // If it's not a primitive, it's a struct, which is also a pointer (i32)
                "i32".to_string()
            }
        };
        Ok(r)
    }

    /// Resolves a possibly-generic type name to its concrete form during code generation,
    /// using the active monomorphization bindings. Handles `T`, `T[]`, and `T?` by stripping
    /// and re-applying the suffix around the bound base type.
    pub fn resolve_type(&self, type_str: &str) -> String {
        let (base, suffix) = if let Some(base) = type_str.strip_suffix("[]") {
            (base, "[]")
        } else if let Some(base) = type_str.strip_suffix('?') {
            (base, "?")
        } else {
            (type_str, "")
        };
        match self.ctx.current_generic_bindings.get(base) {
            Some(concrete) => format!("{}{}", concrete, suffix),
            None => type_str.to_string(),
        }
    }

    /// Reads the type of a variable from the symbol table
    pub fn table_read_type(&self, var_name: &String, function: &FunctionNode<'a>) -> String {
        let func_name = self.ctx.current_mangled_name.as_ref().unwrap_or(&function.name.text);
        let func_lookup = self.ctx.combined_symbol_lookup.get(func_name).unwrap();
        let t = func_lookup.get(var_name).unwrap().clone().get_type();
        self.resolve_type(&t)
    }

    /// Builds local variable declarations for a function
    pub fn build_local_variable(&mut self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let func_name = self.ctx.current_mangled_name.as_ref().unwrap_or(&function.name.text).clone();
        let res = self.get_local_variables(self.symbol_map.get(&func_name).unwrap())?;

        let mut param_names = std::collections::HashSet::new();
        for param in &function.parameters {
            param_names.insert(param.name.text.clone());
        }

        for (name, _type) in res.iter() {
            // Do not emit local variable declarations for function parameters
            if param_names.contains(name) {
                continue;
            }
            let resolved_type = self.resolve_type(&_type.get_type());
            writer.write(" (local ");
            writer.write(&format!("${} {}", name, WasmGenerator::get_wasm_type_from(resolved_type)?));
            writer.write(") ");
        }
        self.ctx.combined_symbol_lookup.insert(func_name, res);
        Ok(())
    }

    /// Gets all local variables from a symbol table and its children
    pub fn get_local_variables(&self, symbol: &Rc<RefCell<SymbolTable>>) -> Result<HashMap<String, Type>, Error> {
        let mut res = HashMap::new();
        let current_scope = (*symbol).as_ref().borrow();
        let mut local_variables = current_scope.get_all();

        for children in current_scope.children.iter() {
            let child_local_variables = self.get_local_variables(children)?;
            local_variables.extend(child_local_variables);
        }
        
        for (name, type_) in local_variables.iter() {
            if !res.contains_key(name) {
                res.insert(name.clone(), type_.clone());
            }
        }

        Ok(res)
    }

    /// Infers the type of an expression (simplified version of semantic analyzer)
    pub fn infer_expression_type(&self, expression: &crate::syntax::nodes::ExpressionNode<'a>, function: &FunctionNode<'a>) -> Result<String, Error> {
        use crate::syntax::nodes::ExpressionNode;
        match expression {
            ExpressionNode::Literal(t) => Ok(self.resolve_type(&t.get_type())),
            ExpressionNode::Identifier(id) => Ok(self.table_read_type(&id.text, function)),
            ExpressionNode::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    Ok("void[]".to_string())
                } else {
                    let inner = self.infer_expression_type(&elements[0], function)?;
                    Ok(format!("{}[]", inner))
                }
            },
            ExpressionNode::IndexAccess(arr, _) => {
                let arr_type = self.infer_expression_type(arr, function)?;
                if arr_type.ends_with("[]") {
                    Ok(arr_type[0..arr_type.len()-2].to_string())
                } else {
                    Ok("void".to_string())
                }
            },
            ExpressionNode::FunctionCall(name, generic_args, args) => {
                match name.text.as_str() {
                    "to_string" => return Ok("string".to_string()),
                    "hash_code" => return Ok("int".to_string()),
                    "print" | "println" => return Ok("void".to_string()),
                    "array_new" => {
                        let element = generic_args.as_ref()
                            .and_then(|g| g.first())
                            .map(|t| self.resolve_type(&t.get_type()))
                            .unwrap_or_else(|| "int".to_string());
                        return Ok(format!("{}[]", element));
                    },
                    _ => {}
                }
                // Indirect call through a function-typed local: result is the signature's return.
                if let Some((_, ret)) = self.function_typed_local(&name.text, function) {
                    return Ok(ret.get_type());
                }
                let resolved_name = self.resolve_call_name(&name.text, generic_args, args, function);
                if let Ok(func) = self.function_table.get_function(&resolved_name) {
                    if let Some(ret_type) = &func.return_type {
                        Ok(ret_type.get_type())
                    } else {
                        Ok("void".to_string())
                    }
                } else if self.struct_table.get_struct(&self.constructor_struct_name(&name.text, generic_args)).is_some() {
                    // Constructor call yields the (monomorphized) struct type.
                    Ok(self.constructor_struct_name(&name.text, generic_args))
                } else {
                    // Check stdlib
                    for std_func in crate::stdlib::StdlibFunction::get_all() {
                        if std_func.name == name.text {
                            if let Some(ret_type) = &std_func.return_type {
                                return Ok(ret_type.get_type());
                            } else {
                                return Ok("void".to_string());
                            }
                        }
                    }
                    Ok("void".to_string())
                }
            },
            ExpressionNode::Unary(_, right) => self.infer_expression_type(right, function),
            // `await e` unwraps a `Future<T>` to `T`. In codegen, async functions are registered
            // in the function table under their declared return type `T`, so the awaited call's
            // inferred type is already `T`.
            ExpressionNode::Await(inner) => self.infer_expression_type(inner, function),
            ExpressionNode::Binary(left, opr, _) => {
                use crate::syntax::token::token_kind::TokenKind;
                match opr.kind {
                    TokenKind::EqualEqualToken | TokenKind::NotEqualToken |
                    TokenKind::GreaterThanToken | TokenKind::SmallerThanToken |
                    TokenKind::GreaterThanEqualToken | TokenKind::SmallerThanEqualToken |
                    TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken => Ok("bool".to_string()),
                    // `a ?? b` yields the unwrapped (non-nullable) element type of `a`.
                    TokenKind::QuestionQuestionToken => {
                        let left_type = self.infer_expression_type(left, function)?;
                        Ok(left_type.trim_end_matches('?').to_string())
                    },
                    _ => self.infer_expression_type(left, function)
                }
            },
            ExpressionNode::Parenthesized(expr) => self.infer_expression_type(expr, function),
            ExpressionNode::Cast(target_type, _) => Ok(target_type.get_type()),
            ExpressionNode::StructInstantiation(name, generic_args, _) => {
                let struct_name = match generic_args {
                    Some(args) => mangle_with_suffixes(&name.text, args.iter().map(|arg| self.resolve_type(&arg.get_type()))),
                    None => name.text.clone(),
                };
                Ok(struct_name)
            },
            ExpressionNode::MemberAccess(obj, member) => {
                // Enum member access yields the enum type (an i32 at runtime).
                if let ExpressionNode::Identifier(id) = obj {
                    if self.enums.contains_key(&id.text) {
                        return Ok(id.text.clone());
                    }
                }
                let obj_type = self.infer_expression_type(obj, function)?;
                // A field may be accessed through a nullable handle (`node.value` where
                // `node: Node?`); resolve the underlying struct layout.
                if let Some(struct_info) = self.struct_table.get_struct(strip_nullable(&obj_type)) {
                    if let Some(field_info) = struct_info.fields.get(&member.text) {
                        return Ok(field_info.type_.get_type());
                    }
                }
                Ok("void".to_string())
            },
            ExpressionNode::IsExpression(_, _) => Ok("bool".to_string()),
            ExpressionNode::Ternary(_, then_e, _) => self.infer_expression_type(then_e, function),
            ExpressionNode::MethodCall(obj, method, generic_args, params) => {
                if let ExpressionNode::Identifier(id) = obj {
                    if id.text == "Math" {
                        return Ok("float".to_string());
                    }
                    // `JSON.serialize(x): string` and `JSON.deserialize<T>(text): T` intrinsics.
                    if id.text == "JSON" && (method.text == "serialize" || method.text == "serialize_pretty") {
                        return Ok("string".to_string());
                    }
                    if id.text == "JSON" && method.text == "deserialize" {
                        return Ok(generic_args.as_ref()
                            .and_then(|g| g.first())
                            .map(|t| self.resolve_type(&t.get_type()))
                            .unwrap_or_else(|| "object".to_string()));
                    }
                }
                if let Some(key) = self.resolve_static_call(obj, &method.text, params, function) {
                    if let Ok(func_info) = self.function_table.get_function(&key) {
                        return Ok(func_info.return_type.as_ref().map(|r| r.get_type()).unwrap_or_else(|| "void".to_string()));
                    }
                }
                let obj_type = self.infer_expression_type(obj, function)?;
                let struct_name = strip_nullable(&obj_type).to_string();
                // `arr.len()` / `str.len()` always yield int.
                if method.text == "len" && (struct_name.ends_with("[]") || struct_name == "string") {
                    return Ok("int".to_string());
                }
                // `EnumValue.name()` yields the variant name as a string.
                if method.text == "name" && self.enums.contains_key(&struct_name) {
                    return Ok("string".to_string());
                }
                let mangled_name = self.resolve_method_key(&struct_name, &method.text, params, function);
                if let Ok(func_info) = self.function_table.get_function(&mangled_name) {
                    if let Some(ret) = &func_info.return_type {
                        return Ok(ret.get_type());
                    }
                }
                Ok("void".to_string())
            },
        }
    }
}
