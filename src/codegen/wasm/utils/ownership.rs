//! Reference-ownership classification used to keep reference counts balanced: deciding which
//! expressions yield an *owned* (+1) reference vs. a *borrowed* one, how call arguments are
//! retained/released around a call, and what a method call returns.

use std::io::Error;
use crate::syntax::nodes::FunctionNode;
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use crate::intrinsics;
use super::super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
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
            ExpressionNode::MethodCall(_, method, _, _) => method.text != intrinsics::ENUM_NAME,
            // String concatenation allocates a fresh string; comparison/logical operators yield
            // non-reference values (irrelevant to ownership).
            ExpressionNode::Binary(_, _, _) => true,
            ExpressionNode::FunctionCall(n, generic_args, args) => {
                // `array_new<T>(n)` allocates a fresh array.
                if n.text == intrinsics::ARRAY_NEW {
                    return true;
                }
                // `print`/`println`/`hash_code` never yield an owned reference. `to_string` may
                // pass through a borrowed string (string argument), so treat it conservatively as
                // borrowed (at worst leaks the freshly formatted string, never over-releases).
                if intrinsics::is_object_builtin(&n.text) {
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
            if id.text == intrinsics::MATH {
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
        if method.text == intrinsics::LEN && (struct_name.ends_with("[]") || struct_name == "string") {
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
            if id.text == intrinsics::MATH {
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
        if method.text == intrinsics::LEN && (struct_name.ends_with("[]") || struct_name == "string") {
            return Ok(Some("int".to_string()));
        }
        if method.text == intrinsics::ENUM_NAME && self.enums.contains_key(&struct_name) {
            return Ok(Some("string".to_string()));
        }
        let mangled_name = self.resolve_method_key(&struct_name, &method.text, params, function);
        Ok(self.function_table.get_function(&mangled_name)
            .ok()
            .and_then(|info| info.return_type)
            .map(|ret| ret.get_type()))
    }
}
