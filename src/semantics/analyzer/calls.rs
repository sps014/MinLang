//! Analysis of call expressions: free-function and overload resolution, method calls, static /
//! namespaced calls (`Math.*` / `JSON.*` / async intrinsics / `derive` helpers), and constructors.

use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::intrinsics;
use crate::semantics::function_table::{
    overload_arg_compatible, FunctionTableInfo, OverloadResolution,
};
use crate::semantics::symbol_table::SymbolTable;
use crate::syntax::nodes::types::{
    canonical_type_name, constructor_fn, is_numeric_primitive, is_unknown_type_name,
    mangle_generic, method_fn, numeric_widen, strip_nullable,
};
use crate::syntax::nodes::{ExpressionNode, FunctionNode, Type};
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// Resolves an overloaded base name against the concrete `arg_types`, returning the selected
    /// signature or a human-readable error (no match / ambiguous). Used by both free-function and
    /// method call analysis (methods prepend the receiver type as the implicit `this` argument).
    pub(super) fn select_function_overload(
        &self,
        base: &str,
        arg_types: &[String],
    ) -> Result<FunctionTableInfo, String> {
        let compat = |param: &str, arg: &str| {
            overload_arg_compatible(param, arg, |t| self.enum_table.contains_key(t))
        };
        match self.function_table.select_overload(base, arg_types, compat) {
            OverloadResolution::Unique(key) => Ok(self.function_table.get_function(&key).unwrap()),
            OverloadResolution::None => Err(format!(
                "No overload of '{}' matches argument types ({})",
                base,
                arg_types.join(", ")
            )),
            OverloadResolution::Ambiguous(keys) => Err(format!(
                "Ambiguous call to '{}' with argument types ({}); candidates: {}",
                base,
                arg_types.join(", "),
                keys.join(", ")
            )),
        }
    }

    /// Analyzes a static-method call `Type.method(args)` (resolved by the caller to the type
    /// `type_name`). Static methods have no implicit `this`, so the explicit arguments map 1:1 to
    /// the declared parameters.
    pub(super) fn analyze_static_call(
        &mut self,
        type_name: &str,
        method: &SyntaxToken,
        params: &Vec<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        let base = method_fn(type_name, &method.text);

        let mut arg_types = Vec::new();
        for param in params.iter() {
            arg_types.push(
                self.analyze_expression(param, parent_function, symbol_table, diagnostics)?
                    .get_type(),
            );
        }

        let store_sig = if self.function_table.is_overloaded(&base) {
            match self.select_function_overload(&base, &arg_types) {
                Ok(sig) => sig,
                Err(message) => {
                    diagnostics.report_error(message, Some(method.position));
                    return Ok(Type::Unknown);
                }
            }
        } else {
            match self.function_table.get_function(&base) {
                Ok(s) => s.clone(),
                Err(_) => {
                    diagnostics.report_error(
                        format!(
                            "Type '{}' has no static method '{}'",
                            type_name, method.text
                        ),
                        Some(method.position),
                    );
                    return Ok(Type::Unknown);
                }
            }
        };

        if !store_sig.is_public && !self.in_methods_of(parent_function, type_name) {
            diagnostics.report_error(
                format!("'{}' is private to '{}'", method.text, type_name),
                Some(method.position),
            );
        }

        let expected_params = store_sig.parameters.clone();
        if expected_params.len() != arg_types.len() {
            diagnostics.report_error(
                format!(
                    "static method {} expects {} parameters, got {}",
                    base,
                    expected_params.len(),
                    arg_types.len()
                ),
                Some(method.position),
            );
            return Ok(store_sig.return_type.unwrap_or(Type::Void));
        }
        for (i, given_type) in arg_types.iter().enumerate() {
            let expected = &expected_params[i];
            if expected == "object" || is_unknown_type_name(given_type) {
                continue;
            }
            if is_numeric_primitive(expected) && is_numeric_primitive(given_type) {
                continue;
            }
            if given_type != expected {
                diagnostics.report_error(
                    format!(
                        "static method {} expects parameter {} to be {}, got {}",
                        base,
                        i + 1,
                        expected,
                        given_type
                    ),
                    Some(method.position),
                );
            }
        }

        // An async static method (e.g. `File.read`) eagerly starts a task; the call yields a
        // `Future<T>` that must be `await`ed, just like an async instance method or free function.
        if store_sig.is_async {
            return Ok(Self::future_type(
                store_sig.return_type.unwrap_or(Type::Void),
            ));
        }

        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }

    /// True when `parent_function` is a method whose implicit `this` receiver has base type
    /// `base_name` (allowing for monomorphized generic variants). Used to gate access to
    /// `_`-prefixed (private) members.
    pub(super) fn in_methods_of(
        &self,
        parent_function: &FunctionNode<'a>,
        base_name: &str,
    ) -> bool {
        // A `static` method belongs to its declaring type, so it may access that type's private
        // members even though it has no `this` receiver. Static methods are registered under the
        // mangled name `{Type}_{method}`, so a name prefixed with `{base_name}_` identifies one.
        if parent_function.is_static {
            let name = &parent_function.name.text;
            return name == base_name
                || name.starts_with(&format!("{}_", base_name))
                || base_name.starts_with(&format!("{}_", name));
        }
        let Some(first) = parent_function.parameters.first() else {
            return false;
        };
        if first.name.text != "this" {
            return false;
        }
        let this_base = Self::resolve_struct_parts(&first.type_)
            .map(|(b, _)| b)
            .unwrap_or_else(|| strip_nullable(&first.type_.get_type()).to_string());
        this_base == base_name
            || this_base.starts_with(&format!("{}_", base_name))
            || base_name.starts_with(&format!("{}_", this_base))
    }

    pub(super) fn analyze_function_call(
        &mut self,
        name: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        let mut function_name = name.text.clone();
        let mut params_types = vec![];
        for param in params.iter() {
            params_types.push(
                self.analyze_expression(param, parent_function, symbol_table, diagnostics)?
                    .get_type(),
            );
        }

        // Indirect call: if the called name is a local variable of function type, validate the
        // arguments against the function-type signature and return its result type.
        if let Ok(Type::Function(param_types, ret)) =
            (*symbol_table).as_ref().borrow().get_symbol(name)
        {
            if param_types.len() != params_types.len() {
                diagnostics.report_error(
                    format!(
                        "function value '{}' expects {} arguments, got {}",
                        name.text,
                        param_types.len(),
                        params_types.len()
                    ),
                    Some(name.position),
                );
                return Ok((*ret).clone());
            }
            for i in 0..param_types.len() {
                let expected = param_types[i].get_type();
                if expected != "object" && expected != params_types[i] {
                    diagnostics.report_error(
                        format!(
                            "function value '{}' expects argument {} to be {}, got {}",
                            name.text,
                            i + 1,
                            expected,
                            params_types[i]
                        ),
                        Some(name.position),
                    );
                }
            }
            return Ok((*ret).clone());
        }

        // Constructor call: `Struct(args)` / `Struct<T>(args)`. Only treated as a constructor
        // when no free function (concrete or generic) shadows the name, so prelude factory
        // functions such as `List<T>()` keep their behaviour.
        if self.function_table.get_function(&function_name).is_err()
            && !self.function_table.is_overloaded(&function_name)
            && !self.generic_functions.contains_key(&function_name)
            && (self.struct_table.get_struct(&function_name).is_some()
                || self.generic_structs.contains_key(&function_name))
        {
            return self.analyze_constructor_call(name, generic_args, &params_types, diagnostics);
        }

        // Monomorphization: bind every generic parameter to a concrete type, then register
        // (once) a specialized signature under the mangled name.
        if self.generic_functions.contains_key(&function_name) {
            let template = *self.generic_functions.get(&function_name).unwrap();
            let bindings = self.infer_generic_bindings(
                template,
                generic_args,
                &params_types,
                &name.position,
                diagnostics,
            );
            let mangled_name = mangle_bindings(&function_name, &bindings);

            if self.function_table.get_function(&mangled_name).is_err() {
                // Store a clone with its signature monomorphized (params + return type made
                // concrete), mirroring how struct methods are specialized. The body is shared and
                // resolved against the bindings during analysis/codegen, so the declared return
                // type (e.g. `List<T>` -> `List_int`) stays consistent with what the body builds.
                let mut specialized = template.clone();
                Self::substitute_generic_signature(&mut specialized, &bindings);
                let specialized_ref: &'a FunctionNode<'a> = self.arena.alloc(specialized);
                self.instantiated_generics
                    .insert(mangled_name.clone(), (bindings.clone(), specialized_ref));

                let info = FunctionTableInfo {
                    name: mangled_name.clone(),
                    parameters: template
                        .parameters
                        .iter()
                        .map(|p| Self::monomorphize_type(&p.type_, &bindings).get_type())
                        .collect(),
                    return_type: template
                        .return_type
                        .as_ref()
                        .map(|ret| Self::monomorphize_type(ret, &bindings)),
                    is_async: template.is_async,
                    is_public: template.is_public,
                    intrinsic_name: intrinsics::intrinsic_key(&template.attributes),
                };

                let _ = self.function_table.add_function(mangled_name.clone(), info);
            }
            function_name = mangled_name;
        }

        // Overloaded free functions resolve by argument types; non-overloaded names keep the
        // direct single-signature lookup (and its precise per-argument diagnostics below).
        let store_sig = if self.function_table.is_overloaded(&function_name) {
            match self.select_function_overload(&function_name, &params_types) {
                Ok(sig) => sig,
                Err(message) => {
                    diagnostics.report_error(message, Some(name.position));
                    return Ok(Type::Unknown);
                }
            }
        } else {
            match self.function_table.get_function(&function_name) {
                Ok(sig) => sig,
                Err(e) => {
                    diagnostics.report_error(e.to_string(), Some(name.position));
                    return Ok(Type::Unknown);
                }
            }
        };

        if store_sig.parameters.len() != params_types.len() {
            diagnostics.report_error(
                format!(
                    "Function {} has {} params but {} params are given",
                    function_name,
                    store_sig.parameters.len(),
                    params_types.len()
                ),
                Some(name.position),
            );
            return Ok(Type::Void);
        }

        for i in 0..params_types.len() {
            // A parameter declared `object` accepts any argument type (boxing happens in codegen).
            if store_sig
                .parameters
                .get(i)
                .map(|s| s == "object")
                .unwrap_or(false)
                || params_types
                    .get(i)
                    .map(|s| is_unknown_type_name(s))
                    .unwrap_or(false)
            {
                continue;
            }
            if store_sig.parameters.get(i) != params_types.get(i) {
                let expected = store_sig
                    .parameters
                    .get(i)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let given = params_types.get(i).map(|s| s.as_str()).unwrap_or("");
                if self.enum_int_compatible(expected, given) {
                    continue;
                }
                diagnostics.report_error(
                    format!(
                        "Function {} has param {} of type {:?} but param {} of type {:?} is given",
                        function_name,
                        i,
                        store_sig.parameters.get(i),
                        i,
                        params_types[i]
                    ),
                    Some(name.position),
                );
            }
        }

        //let r_type=&store_sig.return_type;
        // Calling an `async fun` is eager and yields a `Future<T>` handle (where `T` is the
        // declared return type). It is NOT auto-awaited; `await` retrieves the `T`.
        if store_sig.is_async {
            return Ok(Self::future_type(
                store_sig.return_type.unwrap_or(Type::Void),
            ));
        }
        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }

    /// Types the async intrinsics: `sleep(ms: int): Future<void>`, `all(xs: Future<T>[]):
    /// Future<T[]>`, `any`/`race(xs: Future<T>[]): Future<T>`.
    pub(super) fn analyze_async_intrinsic(
        &mut self,
        name: &SyntaxToken,
        params: &Vec<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        if name.text == intrinsics::SLEEP {
            if params.len() != 1 {
                diagnostics.report_error(
                    format!(
                        "'sleep' expects exactly 1 argument (milliseconds), got {}",
                        params.len()
                    ),
                    Some(name.position),
                );
            }
            for p in params {
                let pt = self.analyze_expression(p, parent_function, symbol_table, diagnostics)?;
                if pt.get_type() != "int" {
                    diagnostics.report_error(
                        format!("'sleep' expects an int argument, got {}", pt.get_type()),
                        p.position(),
                    );
                }
            }
            return Ok(Self::future_type(Type::Void));
        }

        // all/any/race take a single `Future<T>[]` argument.
        if params.len() != 1 {
            diagnostics.report_error(
                format!(
                    "'{}' expects exactly 1 argument (a Future array), got {}",
                    name.text,
                    params.len()
                ),
                Some(name.position),
            );
            return Ok(Self::future_type(Type::Void));
        }
        let arg_type =
            self.analyze_expression(&params[0], parent_function, symbol_table, diagnostics)?;
        let inner_t = match &arg_type {
            Type::Array(inner) => match Self::future_inner_type(inner) {
                Some(t) => t,
                None => {
                    diagnostics.report_error(
                        format!(
                            "'{}' expects an array of Future values, got {}",
                            name.text,
                            arg_type.get_type()
                        ),
                        params[0].position(),
                    );
                    Type::Void
                }
            },
            _ => {
                diagnostics.report_error(
                    format!(
                        "'{}' expects an array of Future values, got {}",
                        name.text,
                        arg_type.get_type()
                    ),
                    params[0].position(),
                );
                Type::Void
            }
        };
        if name.text == intrinsics::PROMISE_ALL {
            // Future<T[]>
            Ok(Self::future_type(Type::Array(Box::new(inner_t))))
        } else {
            // any / race -> Future<T>
            Ok(Self::future_type(inner_t))
        }
    }

    /// String-level assignability check for argument vs. parameter/field types, mirroring the
    /// rules in [`compare_data_type`] (which works on `Type`). An `expected` type accepts a `given`
    /// when they are identical, the target is `object`, they are enum/int compatible, or the target
    /// is nullable (`T?`) and the argument is `T`, `T?`, or the `null` literal (`void?`). Used by
    /// constructor-call checking, which only has the type names (not structured `Type`s) available.
    pub(super) fn type_str_assignable(&self, expected: &str, given: &str) -> bool {
        // The poison type unifies with everything so an earlier error never cascades into a
        // spurious assignment/argument mismatch here.
        if crate::syntax::nodes::types::is_unknown_type_name(expected)
            || crate::syntax::nodes::types::is_unknown_type_name(given)
        {
            return true;
        }
        if expected == given || expected == "object" {
            return true;
        }
        if self.enum_int_compatible(expected, given) {
            return true;
        }
        // Implicit numeric widening: a narrower numeric argument may satisfy a wider numeric
        // parameter/field without a cast (narrowing still requires one).
        if numeric_widen(strip_nullable(given), strip_nullable(expected)) {
            return true;
        }
        if let Some(inner) = expected.strip_suffix('?') {
            if given == "void?" {
                return true;
            }
            let given_inner = given.strip_suffix('?').unwrap_or(given);
            if inner == given_inner {
                return true;
            }
        }
        // Any reference (or nullable) target accepts the `null` literal.
        (expected.ends_with('?') || self.is_reference_type(expected)) && given == "void?"
    }

    /// Type-checks a constructor call `Struct(args)`. When the struct defines a custom `constructor`
    /// the call is checked against `init`'s parameters; otherwise it is checked positionally
    /// against the struct's fields in declaration order (the auto-generated constructor).
    pub(super) fn analyze_constructor_call(
        &mut self,
        name: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params_types: &[String],
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        let struct_name = match generic_args {
            Some(args) if !args.is_empty() => {
                self.ensure_struct_instantiated(&name.text, args, &name.position, diagnostics);
                mangle_generic(&name.text, args)
            }
            _ => {
                if self.generic_structs.contains_key(&name.text) {
                    diagnostics.report_error(
                        format!(
                            "Generic class '{}' requires type arguments, e.g. {}<int>(...)",
                            name.text, name.text
                        ),
                        Some(name.position),
                    );
                }
                name.text.clone()
            }
        };

        let init_name = constructor_fn(&struct_name);
        let expected: Vec<String> = if let Ok(sig) = self.function_table.get_function(&init_name) {
            // `constructor` is registered as a method, so parameter 0 is the implicit `this`.
            sig.parameters.iter().skip(1).cloned().collect()
        } else if let Some(info) = self.struct_table.get_struct(&struct_name) {
            let mut ordered: Vec<(&String, &crate::semantics::struct_table::StructFieldInfo)> =
                info.fields.iter().collect();
            ordered.sort_by_key(|(_, f)| f.offset);
            ordered.iter().map(|(_, f)| f.type_.get_type()).collect()
        } else {
            Vec::new()
        };

        if expected.len() != params_types.len() {
            diagnostics.report_error(
                format!(
                    "Constructor for '{}' expects {} argument(s), but {} were given",
                    struct_name,
                    expected.len(),
                    params_types.len()
                ),
                Some(name.position),
            );
        } else {
            for i in 0..expected.len() {
                let e = expected[i].as_str();
                let g = params_types[i].as_str();
                if self.type_str_assignable(e, g) {
                    continue;
                }
                diagnostics.report_error(
                    format!(
                        "Constructor for '{}' expects argument {} to be '{}', got '{}'",
                        struct_name,
                        i + 1,
                        e,
                        g
                    ),
                    Some(name.position),
                );
            }
        }

        Ok(Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, &struct_name),
            None,
        ))
    }

    pub(super) fn analyze_method_call(
        &mut self,
        obj: &ExpressionNode<'a>,
        method: &SyntaxToken,
        _generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        if let ExpressionNode::Identifier(id) = obj {
            // `Type.method(args)`: a static-method call. The receiver names a type (not a local
            // variable), so resolve `{type}_{method}` directly with no implicit `this`.
            let is_local = (*ctx.symbol_table).as_ref().borrow().get_symbol(id).is_ok();
            if !is_local {
                // `Enum.Variant(args)`: construct a discriminated-union value.
                if let Some(t) = self.analyze_variant_construction(
                    &id.text,
                    method,
                    params,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )? {
                    return Ok(t);
                }
            }
            if !is_local {
                let type_name = canonical_type_name(&id.text)
                    .unwrap_or(id.text.as_str())
                    .to_string();
                let base = method_fn(&type_name, &method.text);

                // Support generic static method calls by monomorphizing them on the fly.
                if self.generic_functions.contains_key(&base) {
                    let template = *self.generic_functions.get(&base).unwrap();
                    let mut params_types = vec![];
                    for param in params.iter() {
                        params_types.push(
                            self.analyze_expression(
                                param,
                                ctx.parent_function,
                                ctx.symbol_table,
                                diagnostics,
                            )?
                            .get_type(),
                        );
                    }
                    // `Array.new<T>(len)`: a generic intrinsic that allocates a zero-initialized
                    // `T[]`. The element type comes from the explicit type argument (resolved
                    // through the active monomorphization bindings so `Array.new<T>` inside a
                    // `List<int>` method yields `int[]`).
                    if intrinsics::IntrinsicOp::from_attributes(&template.attributes)
                        == Some(intrinsics::IntrinsicOp::ArrayNew)
                    {
                        let element = match _generic_args.as_ref().and_then(|g| g.first()) {
                            Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                            None => {
                                diagnostics.report_error(
                                    "'Array.new' requires a type argument, e.g. Array.new<int>(n)"
                                        .to_string(),
                                    Some(method.position),
                                );
                                Type::Void
                            }
                        };
                        if params_types.len() != 1 {
                            diagnostics.report_error(
                                format!(
                                    "'Array.new' expects exactly 1 argument (length), got {}",
                                    params_types.len()
                                ),
                                Some(method.position),
                            );
                        } else if params_types[0] != "int"
                            && !is_unknown_type_name(&params_types[0])
                        {
                            diagnostics.report_error(
                                format!("'Array.new' length must be int, got {}", params_types[0]),
                                Some(method.position),
                            );
                        }
                        return Ok(Type::Array(Box::new(element)));
                    }

                    let bindings = self.infer_generic_bindings(
                        template,
                        _generic_args,
                        &params_types,
                        &method.position,
                        diagnostics,
                    );
                    let mangled_name = mangle_bindings(&base, &bindings);

                    // Promise combinators (`Promise.all/any/race`) are typed by the shared async
                    // intrinsic logic; classify via the registry and delegate when applicable.
                    if let Some(combinator) =
                        intrinsics::IntrinsicOp::from_attributes(&template.attributes)
                            .and_then(|op| op.promise_combinator())
                    {
                        let mut s_tok = method.clone();
                        s_tok.text = combinator.to_string();
                        return self.analyze_async_intrinsic(
                            &s_tok,
                            params,
                            ctx.parent_function,
                            ctx.symbol_table,
                            diagnostics,
                        );
                    }

                    if self.function_table.get_function(&mangled_name).is_err() {
                        let mut specialized = template.clone();
                        Self::substitute_generic_signature(&mut specialized, &bindings);
                        let specialized_ref: &'a FunctionNode<'a> = self.arena.alloc(specialized);
                        let info = FunctionTableInfo::from(specialized_ref);
                        self.function_table
                            .add_function(mangled_name.clone(), info)
                            .unwrap();
                        self.instantiated_generics
                            .insert(mangled_name.clone(), (bindings, specialized_ref));
                    }
                    let info = self.function_table.get_function(&mangled_name).unwrap();
                    if info.is_async {
                        return Ok(Self::future_type(info.return_type.unwrap_or(Type::Void)));
                    }
                    return Ok(info.return_type.unwrap_or(Type::Void));
                }

                if self.function_table.is_overloaded(&base)
                    || self.function_table.get_function(&base).is_ok()
                {
                    return self.analyze_static_call(
                        &type_name,
                        method,
                        params,
                        ctx.parent_function,
                        ctx.symbol_table,
                        diagnostics,
                    );
                }
            }
        }

        let obj_type =
            self.analyze_expression(obj, ctx.parent_function, ctx.symbol_table, diagnostics)?;

        // The receiver was already poisoned by an earlier error; still type-check the arguments
        // (to surface their own mistakes) but stay quiet about the method itself and stay poison.
        if obj_type.is_unknown() {
            for param in params.iter() {
                let _ = self.analyze_expression(
                    param,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                );
            }
            return Ok(Type::Unknown);
        }

        // `EnumValue.name()`: built-in accessor returning the variant name as a string.
        if method.text == intrinsics::ENUM_NAME {
            let base = strip_nullable(&obj_type.get_type()).to_string();
            if self.enum_table.contains_key(&base) {
                if !params.is_empty() {
                    diagnostics.report_error(
                        format!("'name' takes no arguments, got {}", params.len()),
                        Some(method.position),
                    );
                }
                return Ok(Type::String(synthetic_token(
                    TokenKind::DataTypeToken,
                    "string",
                )));
            }
        }

        // `arr.len()` / `str.len()`: built-in length method on arrays and strings.
        if method.text == intrinsics::LEN {
            let base = strip_nullable(&obj_type.get_type()).to_string();
            if base.ends_with("[]") || base == "string" {
                if !params.is_empty() {
                    diagnostics.report_error(
                        format!("'len' takes no arguments, got {}", params.len()),
                        Some(method.position),
                    );
                }
                return Ok(Type::Integer(synthetic_token(
                    TokenKind::DataTypeToken,
                    "int",
                )));
            }
        }

        // `str.char_at(i)`: built-in character accessor on strings (low-level read).
        if method.text == intrinsics::CHAR_AT && strip_nullable(&obj_type.get_type()) == "string" {
            if params.len() != 1 {
                diagnostics.report_error(
                    format!(
                        "'char_at' expects exactly 1 argument (index), got {}",
                        params.len()
                    ),
                    Some(method.position),
                );
            }
            for param in params.iter() {
                let pt = self.analyze_expression(
                    param,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )?;
                if pt.get_type() != "int" && !is_unknown_type_name(&pt.get_type()) {
                    diagnostics.report_error(
                        format!("'char_at' index must be int, got {}", pt.get_type()),
                        param.position(),
                    );
                }
            }
            return Ok(Type::Char(synthetic_token(
                TokenKind::DataTypeToken,
                "char",
            )));
        }

        // Object protocol: `x.to_string()` / `x.hash_code()` are available on every type. A
        // user-defined override (registered as `{Type}_to_string`) takes precedence and is resolved
        // by the normal method lookup below; otherwise fall back to the builtin protocol.
        if method.text == intrinsics::TO_STRING || method.text == intrinsics::HASH_CODE {
            let receiver = match Self::resolve_struct_parts(&obj_type) {
                Some((base_name, generic_args)) => mangle_generic(&base_name, &generic_args),
                None => strip_nullable(&obj_type.get_type()).to_string(),
            };
            let user_method = method_fn(&receiver, &method.text);
            let has_override = self.function_table.is_overloaded(&user_method)
                || self.function_table.get_function(&user_method).is_ok();
            if !has_override {
                if !params.is_empty() {
                    diagnostics.report_error(
                        format!("'{}' takes no arguments, got {}", method.text, params.len()),
                        Some(method.position),
                    );
                }
                if method.text == intrinsics::TO_STRING {
                    return Ok(Type::String(synthetic_token(
                        TokenKind::DataTypeToken,
                        "string",
                    )));
                }
                return Ok(Type::Integer(synthetic_token(
                    TokenKind::DataTypeToken,
                    "int",
                )));
            }
        }

        // Struct receivers are monomorphized to their concrete type name; primitive/`object`
        // receivers (which can carry methods via `extend`) use their canonical type name directly.
        let struct_name = match Self::resolve_struct_parts(&obj_type) {
            Some((base_name, generic_args)) => {
                // A generic union receiver (e.g. `Option<int>`) is instantiated through the union
                // path so its extension methods are registered; everything else is a struct.
                self.ensure_type_instantiated(
                    &base_name,
                    &generic_args,
                    &method.position,
                    diagnostics,
                );
                mangle_generic(&base_name, &generic_args)
            }
            None => strip_nullable(&obj_type.get_type()).to_string(),
        };

        let mangled_name = method_fn(&struct_name, &method.text);

        // Analyze the explicit arguments once, then resolve the method (overloaded methods select
        // by argument types, with the receiver supplied as the implicit `this` argument).
        let mut arg_types = Vec::new();
        for param in params.iter() {
            arg_types.push(
                self.analyze_expression(param, ctx.parent_function, ctx.symbol_table, diagnostics)?
                    .get_type(),
            );
        }

        let store_sig = if self.function_table.is_overloaded(&mangled_name) {
            let mut selection_args = Vec::with_capacity(arg_types.len() + 1);
            selection_args.push(struct_name.clone());
            selection_args.extend(arg_types.iter().cloned());
            match self.select_function_overload(&mangled_name, &selection_args) {
                Ok(sig) => sig,
                Err(message) => {
                    diagnostics.report_error(message, Some(method.position));
                    return Ok(Type::Unknown);
                }
            }
        } else {
            match self.function_table.get_function(&mangled_name) {
                Ok(s) => s.clone(),
                Err(_) => {
                    diagnostics.report_error(
                        format!("Type '{}' has no method '{}'", struct_name, method.text),
                        Some(method.position),
                    );
                    return Ok(Type::Unknown);
                }
            }
        };

        // Private methods (the default) may only be called from within the declaring type's own
        // methods; `public` exposes them to outside code.
        if !store_sig.is_public {
            let base_name = Self::resolve_struct_parts(&obj_type)
                .map(|(b, _)| b)
                .unwrap_or_else(|| strip_nullable(&obj_type.get_type()).to_string());
            if !self.in_methods_of(ctx.parent_function, &base_name) {
                diagnostics.report_error(
                    format!("'{}' is private to '{}'", method.text, base_name),
                    Some(method.position),
                );
            }
        }

        let mut expected_params = store_sig.parameters.clone();

        // Remove 'this' from the expected params check since we supply it implicitly
        if !expected_params.is_empty() {
            expected_params.remove(0);
        }

        if expected_params.len() != arg_types.len() {
            diagnostics.report_error(
                format!(
                    "function {} expects {} parameters, got {}",
                    mangled_name,
                    expected_params.len(),
                    arg_types.len()
                ),
                Some(method.position),
            );
            return Ok(store_sig.return_type.unwrap_or(Type::Void));
        }

        for (i, given_type) in arg_types.iter().enumerate() {
            let expected_type_str = &expected_params[i];

            if expected_type_str == "object" || is_unknown_type_name(given_type) {
                continue;
            }

            if is_numeric_primitive(expected_type_str) && is_numeric_primitive(given_type) {
                continue;
            }

            if given_type != expected_type_str {
                diagnostics.report_error(
                    format!(
                        "function {} expects parameter {} to be {}, got {}",
                        mangled_name,
                        i + 1,
                        expected_type_str,
                        given_type
                    ),
                    Some(method.position),
                );
            }
        }

        // Calling an `async` method is eager and yields a `Future<T>` handle (like free async
        // functions); `await` retrieves the `T`.
        if store_sig.is_async {
            return Ok(Self::future_type(
                store_sig.return_type.unwrap_or(Type::Void),
            ));
        }
        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }
}
