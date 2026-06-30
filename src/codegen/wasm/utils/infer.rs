//! Codegen-side, best-effort expression type inference. This mirrors a subset of the semantic
//! analyzer's typing rules so the backend can decide boxing, refcounting, and call resolution
//! without re-running full analysis.

use super::super::WasmGenerator;
use crate::intrinsics;
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::nodes::FunctionNode;
use std::io::Error;

impl<'a> WasmGenerator<'a> {
    /// Infers the type of an expression (simplified version of semantic analyzer)
    pub fn infer_expression_type(
        &self,
        expression: &crate::syntax::nodes::ExpressionNode<'a>,
        function: &FunctionNode<'a>,
    ) -> Result<String, Error> {
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
            }
            ExpressionNode::IndexAccess(arr, _) => {
                let arr_type = self.infer_expression_type(arr, function)?;
                if arr_type.ends_with("[]") {
                    Ok(arr_type[0..arr_type.len() - 2].to_string())
                } else {
                    Ok("void".to_string())
                }
            }
            ExpressionNode::FunctionCall(name, generic_args, args) => {
                if intrinsics::is_object_builtin(&name.text) {
                    return Ok("void".to_string());
                }
                // Indirect call through a function-typed local: result is the signature's return.
                if let Some((_, ret)) = self.function_typed_local(&name.text, function) {
                    return Ok(ret.get_type());
                }
                let resolved_name =
                    self.resolve_call_name(&name.text, generic_args, args, function);
                if let Ok(func) = self.function_table.get_function(&resolved_name) {
                    if let Some(ret_type) = &func.return_type {
                        Ok(ret_type.get_type())
                    } else {
                        Ok("void".to_string())
                    }
                } else if self
                    .struct_table
                    .get_struct(&self.constructor_struct_name(&name.text, generic_args))
                    .is_some()
                {
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
            }
            ExpressionNode::Unary(_, right) => self.infer_expression_type(right, function),
            // `await e` unwraps a `Future<T>` to `T`. In codegen, async functions are registered
            // in the function table under their declared return type `T`, so the awaited call's
            // inferred type is already `T`.
            ExpressionNode::Await(inner) => self.infer_expression_type(inner, function),
            ExpressionNode::Binary(left, opr, _) => {
                use crate::syntax::token::token_kind::TokenKind;
                match opr.kind {
                    TokenKind::EqualEqualToken
                    | TokenKind::NotEqualToken
                    | TokenKind::GreaterThanToken
                    | TokenKind::SmallerThanToken
                    | TokenKind::GreaterThanEqualToken
                    | TokenKind::SmallerThanEqualToken
                    | TokenKind::AmpersandAmpersandToken
                    | TokenKind::PipePipeToken => Ok("bool".to_string()),
                    // `a ?? b` yields the unwrapped (non-nullable) element type of `a`.
                    TokenKind::QuestionQuestionToken => {
                        let left_type = self.infer_expression_type(left, function)?;
                        Ok(left_type.trim_end_matches('?').to_string())
                    }
                    // `+` is string concatenation when either operand is a string (the non-string
                    // side is auto-converted via `to_string`); otherwise it's the left numeric type.
                    TokenKind::PlusToken => {
                        let ExpressionNode::Binary(_, _, right) = expression else {
                            unreachable!()
                        };
                        let left_type = self.infer_expression_type(left, function)?;
                        if strip_nullable(&left_type) == "string" {
                            return Ok("string".to_string());
                        }
                        let right_type = self.infer_expression_type(right, function)?;
                        if strip_nullable(&right_type) == "string" {
                            return Ok("string".to_string());
                        }
                        Ok(left_type)
                    }
                    _ => self.infer_expression_type(left, function),
                }
            }
            ExpressionNode::Parenthesized(expr) => self.infer_expression_type(expr, function),
            ExpressionNode::Cast(target_type, _) => Ok(target_type.get_type()),
            ExpressionNode::MemberAccess(obj, member) => {
                // Unit-variant construction (`Option.None`) yields the union type.
                if let Some(union_name) =
                    self.infer_union_construction(obj, &member.text, &[], function)
                {
                    return Ok(union_name);
                }
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
            }
            ExpressionNode::IsExpression(_, _) => Ok("bool".to_string()),
            ExpressionNode::Ternary(_, then_e, _) => self.infer_expression_type(then_e, function),
            ExpressionNode::Match(_, arms) => {
                // Best-effort: a value-position match's type is its first arm's body type.
                for arm in arms.iter() {
                    if let crate::syntax::nodes::MatchArmBody::Expr(e) = &arm.body {
                        return self.infer_expression_type(e, function);
                    }
                }
                Ok("void".to_string())
            }
            ExpressionNode::MethodCall(obj, method, _generic_args, params) => {
                // Data-variant construction (`Option.Some(42)`) yields the union type.
                if let Some(union_name) =
                    self.infer_union_construction(obj, &method.text, params, function)
                {
                    return Ok(union_name);
                }
                if let Some(key) = self.resolve_static_call(obj, &method.text, params, function) {
                    if let Ok(func_info) = self.function_table.get_function(&key) {
                        return Ok(func_info
                            .return_type
                            .as_ref()
                            .map(|r| r.get_type())
                            .unwrap_or_else(|| "void".to_string()));
                    }
                }
                let obj_type = self.infer_expression_type(obj, function)?;
                let struct_name = strip_nullable(&obj_type).to_string();
                // `arr.len()` / `str.len()` always yield int.
                if method.text == intrinsics::LEN
                    && (struct_name.ends_with("[]") || struct_name == "string")
                {
                    return Ok("int".to_string());
                }
                // `str.char_at(i)` yields a char.
                if method.text == intrinsics::CHAR_AT && struct_name == "string" {
                    return Ok("char".to_string());
                }
                // `EnumValue.name()` yields the variant name as a string.
                if method.text == intrinsics::ENUM_NAME && self.enums.contains_key(&struct_name) {
                    return Ok("string".to_string());
                }
                let mangled_name =
                    self.resolve_method_key(&struct_name, &method.text, params, function);
                if let Ok(func_info) = self.function_table.get_function(&mangled_name) {
                    if let Some(ret) = &func_info.return_type {
                        return Ok(ret.get_type());
                    }
                }
                // Object protocol fallback: `x.to_string()` / `x.hash_code()` on any receiver with
                // no user-defined override.
                if method.text == intrinsics::TO_STRING {
                    return Ok("string".to_string());
                }
                if method.text == intrinsics::HASH_CODE {
                    return Ok("int".to_string());
                }
                Ok("void".to_string())
            }
        }
    }

    /// Best-effort: if `obj.variant(args)` constructs a discriminated-union variant, returns the
    /// concrete (monomorphized) union name. Non-generic unions resolve by name; generic unions are
    /// matched by their instantiated name when unambiguous (or by the first argument's type).
    fn infer_union_construction(
        &self,
        obj: &crate::syntax::nodes::ExpressionNode<'a>,
        variant: &str,
        args: &[crate::syntax::nodes::ExpressionNode<'a>],
        function: &FunctionNode<'a>,
    ) -> Option<String> {
        use crate::syntax::nodes::ExpressionNode;
        let ExpressionNode::Identifier(id) = obj else {
            return None;
        };
        if let Some(info) = self.unions.get(&id.text) {
            if info.variant(variant).is_some() {
                return Some(id.text.clone());
            }
        }
        let prefix = format!("{}_", id.text);
        let candidates: Vec<&crate::semantics::union_table::UnionInfo> = self
            .unions
            .values()
            .filter(|u| u.name.starts_with(&prefix) && u.variant(variant).is_some())
            .collect();
        if candidates.len() == 1 {
            return Some(candidates[0].name.clone());
        }
        if candidates.len() > 1 && !args.is_empty() {
            if let Ok(arg_type) = self.infer_expression_type(&args[0], function) {
                let base = strip_nullable(&arg_type);
                for u in &candidates {
                    if let Some(field) = u.variant(variant).and_then(|v| v.fields.first()) {
                        if strip_nullable(&field.type_.get_type()) == base {
                            return Some(u.name.clone());
                        }
                    }
                }
            }
        }
        None
    }
}
