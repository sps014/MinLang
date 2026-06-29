//! Codegen-side, best-effort expression type inference. This mirrors a subset of the semantic
//! analyzer's typing rules so the backend can decide boxing, refcounting, and call resolution
//! without re-running full analysis.

use super::super::WasmGenerator;
use crate::intrinsics;
use crate::syntax::nodes::types::{mangle_with_suffixes, strip_nullable};
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
                match name.text.as_str() {
                    intrinsics::TO_STRING => return Ok("string".to_string()),
                    intrinsics::HASH_CODE => return Ok("int".to_string()),
                    intrinsics::PRINT | intrinsics::PRINTLN => return Ok("void".to_string()),
                    intrinsics::ARRAY_NEW => {
                        let element = generic_args
                            .as_ref()
                            .and_then(|g| g.first())
                            .map(|t| self.resolve_type(&t.get_type()))
                            .unwrap_or_else(|| "int".to_string());
                        return Ok(format!("{}[]", element));
                    }
                    _ => {}
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
                    _ => self.infer_expression_type(left, function),
                }
            }
            ExpressionNode::Parenthesized(expr) => self.infer_expression_type(expr, function),
            ExpressionNode::Cast(target_type, _) => Ok(target_type.get_type()),
            ExpressionNode::StructInstantiation(name, generic_args, _) => {
                let struct_name = match generic_args {
                    Some(args) => mangle_with_suffixes(
                        &name.text,
                        args.iter().map(|arg| self.resolve_type(&arg.get_type())),
                    ),
                    None => name.text.clone(),
                };
                Ok(struct_name)
            }
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
            }
            ExpressionNode::IsExpression(_, _) => Ok("bool".to_string()),
            ExpressionNode::Ternary(_, then_e, _) => self.infer_expression_type(then_e, function),
            ExpressionNode::MethodCall(obj, method, generic_args, params) => {
                if let ExpressionNode::Identifier(id) = obj {
                    if id.text == intrinsics::MATH {
                        return Ok("float".to_string());
                    }
                    // `JSON.serialize(x): string` and `JSON.deserialize<T>(text): T` intrinsics.
                    if id.text == intrinsics::JSON
                        && (method.text == intrinsics::JSON_SERIALIZE
                            || method.text == intrinsics::JSON_SERIALIZE_PRETTY)
                    {
                        return Ok("string".to_string());
                    }
                    if id.text == intrinsics::JSON && method.text == intrinsics::JSON_DESERIALIZE {
                        return Ok(generic_args
                            .as_ref()
                            .and_then(|g| g.first())
                            .map(|t| self.resolve_type(&t.get_type()))
                            .unwrap_or_else(|| "object".to_string()));
                    }
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
                Ok("void".to_string())
            }
        }
    }
}
