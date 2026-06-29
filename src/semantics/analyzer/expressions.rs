//! Analysis of expressions: expression typing, binary operators, type compatibility checks, and
//! identifier resolution.

use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::semantics::symbol_table::SymbolTable;
use crate::syntax::nodes::types::{is_numeric_primitive, mangle_generic, strip_nullable};
use crate::syntax::nodes::{ExpressionNode, FunctionNode, Type};
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        match expression {
            ExpressionNode::Literal(number) => Ok(number.clone()),
            ExpressionNode::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    diagnostics.report_error(
                        "Empty array literals are not supported yet".to_string(),
                        None,
                    );
                    return Ok(Type::Array(Box::new(Type::Void)));
                }

                let first_type = self.analyze_expression(
                    &elements[0],
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;

                for elem in elements.iter().skip(1) {
                    let element_type =
                        self.analyze_expression(elem, parent_function, symbol_table, diagnostics)?;
                    self.compare_data_type(&first_type, &element_type, &empty_span(), diagnostics)?;
                }

                Ok(Type::Array(Box::new(first_type)))
            }
            ExpressionNode::IndexAccess(array_expr, index_expr) => {
                let array_type = self.analyze_expression(
                    array_expr,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                let inner_type = match array_type {
                    Type::Array(inner) => *inner,
                    _ => {
                        diagnostics.report_error(
                            format!("Cannot index into non-array type {}", array_type.get_type()),
                            array_expr.position(),
                        );
                        Type::Void
                    }
                };

                let index_type = self.analyze_expression(
                    index_expr,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                if index_type.get_type() != "int" {
                    diagnostics.report_error(
                        format!(
                            "Array index must be of type int, got {}",
                            index_type.get_type()
                        ),
                        index_expr.position(),
                    );
                }

                Ok(inner_type)
            }
            ExpressionNode::Unary(opr, right) => {
                let right_type =
                    self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
                match opr.kind {
                    TokenKind::BangToken => {
                        if right_type.get_type() != "bool" {
                            diagnostics.report_error(
                                format!("! operator requires bool, got {}", right_type.get_type()),
                                Some(opr.position),
                            );
                        }
                        Ok(Type::Boolean(opr.clone()))
                    }
                    TokenKind::PlusToken | TokenKind::MinusToken => {
                        if right_type.get_type() != "int" && right_type.get_type() != "float" && right_type.get_type() != "double" {
                            diagnostics.report_error(
                                format!(
                                    "unary +/- requires int, float, or double, got {}",
                                    right_type.get_type()
                                ),
                                Some(opr.position),
                            );
                        }
                        Ok(right_type)
                    }
                    _ => {
                        diagnostics.report_error(
                            format!("unknown unary operator {}", opr.text),
                            Some(opr.position),
                        );
                        Ok(right_type)
                    }
                }
            }
            ExpressionNode::Binary(left, opr, right) => Ok(self.analyze_binary_expression(
                left,
                opr,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?),
            ExpressionNode::Identifier(id) => {
                Ok(self.analyze_identifier(id, symbol_table, diagnostics)?)
            }
            ExpressionNode::FunctionCall(name, generic_args, params) => Ok(self
                .analyze_function_call(
                    name,
                    generic_args,
                    params,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?),
            ExpressionNode::IsExpression(left, _right_type) => {
                // `is` always evaluates to a bool; the actual comparison is resolved at compile time.
                self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                Ok(Type::Boolean(synthetic_token(
                    TokenKind::BooleanToken,
                    "true",
                )))
            }
            ExpressionNode::Parenthesized(expr) => {
                Ok(self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?)
            }
            ExpressionNode::Ternary(condition, then_expr, else_expr) => {
                let cond_type =
                    self.analyze_expression(condition, parent_function, symbol_table, diagnostics)?;
                if cond_type.get_type() != "bool" {
                    diagnostics.report_error(
                        format!(
                            "Ternary condition must be of type bool, got {}",
                            cond_type.get_type()
                        ),
                        condition.position(),
                    );
                }
                let then_type =
                    self.analyze_expression(then_expr, parent_function, symbol_table, diagnostics)?;
                let else_type =
                    self.analyze_expression(else_expr, parent_function, symbol_table, diagnostics)?;
                // Both branches must agree; reuse the standard compatibility check.
                self.compare_data_type(&then_type, &else_type, &empty_span(), diagnostics)?;
                Ok(then_type)
            }
            ExpressionNode::StructInstantiation(name, generic_args, fields) => {
                // Resolve generic type arguments through the active monomorphization bindings so a
                // `List<T>{...}` written inside a generic function/method body instantiates the
                // concrete `List<int>` rather than a stray `List<T>`.
                let resolved_args: Vec<Type> = generic_args
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|a| Self::monomorphize_type(a, &self.current_generic_bindings))
                    .collect();
                let generic_args_slice = resolved_args.as_slice();
                let struct_name = mangle_generic(&name.text, generic_args_slice);

                // Monomorphize generic struct if needed
                self.ensure_struct_instantiated(
                    &name.text,
                    generic_args_slice,
                    &name.position,
                    diagnostics,
                );

                let struct_info = match self.struct_table.get_struct(&struct_name) {
                    Some(info) => info.clone(),
                    None => {
                        diagnostics.report_error(
                            format!("Struct '{}' not found", struct_name),
                            Some(name.position),
                        );
                        return Ok(Type::Void);
                    }
                };

                // Check that all fields are provided and types match
                let mut provided_fields = std::collections::HashSet::new();
                for (field_name, field_expr) in fields {
                    provided_fields.insert(field_name.text.clone());

                    let field_info = match struct_info.fields.get(&field_name.text) {
                        Some(info) => info,
                        None => {
                            diagnostics.report_error(
                                format!(
                                    "Field '{}' not found in class '{}'",
                                    field_name.text, struct_name
                                ),
                                Some(field_name.position),
                            );
                            continue;
                        }
                    };

                    let expr_type = self.analyze_expression(
                        field_expr,
                        parent_function,
                        symbol_table,
                        diagnostics,
                    )?;
                    self.compare_data_type(
                        &field_info.type_,
                        &expr_type,
                        &field_name.position,
                        diagnostics,
                    )?;
                }

                // Check for missing fields
                for expected_field in struct_info.fields.keys() {
                    if !provided_fields.contains(expected_field) {
                        diagnostics.report_error(
                            format!(
                                "Missing field '{}' in class instantiation of '{}'",
                                expected_field, struct_name
                            ),
                            Some(name.position),
                        );
                    }
                }

                let mut dummy_token = name.clone();
                dummy_token.text = struct_name.clone();
                Ok(Type::Struct(dummy_token, None))
            }
            ExpressionNode::MemberAccess(obj, member) => {
                // Enum member access `EnumName.Member` resolves to the enum type (an i32 at runtime).
                if let ExpressionNode::Identifier(id) = obj {
                    if self.enum_table.contains_key(&id.text) {
                        if self.enum_member_value(&id.text, &member.text).is_none() {
                            diagnostics.report_error(
                                format!("Enum '{}' has no member '{}'", id.text, member.text),
                                Some(member.position),
                            );
                        }
                        return Ok(Type::Struct(id.clone(), None));
                    }
                }
                let obj_type =
                    self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

                let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
                    Some(parts) => parts,
                    None => {
                        diagnostics.report_error(
                            format!(
                                "Cannot access member of non-class type {}",
                                obj_type.get_type()
                            ),
                            Some(member.position),
                        );
                        return Ok(Type::Void);
                    }
                };

                self.ensure_struct_instantiated(
                    &base_name,
                    &generic_args,
                    &member.position,
                    diagnostics,
                );
                let struct_name = mangle_generic(&base_name, &generic_args);

                let struct_info = match self.struct_table.get_struct(&struct_name) {
                    Some(info) => info,
                    None => {
                        diagnostics.report_error(
                            format!("Struct '{}' not found", struct_name),
                            Some(member.position),
                        );
                        return Ok(Type::Void);
                    }
                };

                let field_info = match struct_info.fields.get(&member.text) {
                    Some(info) => info,
                    None => {
                        diagnostics.report_error(
                            format!(
                                "Field '{}' not found in class '{}'",
                                member.text, struct_name
                            ),
                            Some(member.position),
                        );
                        return Ok(Type::Void);
                    }
                };

                let field_type = field_info.type_.clone();

                // Private fields (`_name`) may only be read from within the declaring type's methods.
                if member.text.starts_with('_') && !self.in_methods_of(parent_function, &base_name)
                {
                    diagnostics.report_error(
                        format!("'{}' is private to '{}'", member.text, base_name),
                        Some(member.position),
                    );
                }

                Ok(field_type)
            }
            ExpressionNode::Cast(target_type, expr) => {
                let expr_type =
                    self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?;

                let target_type_str = target_type.get_type();
                let expr_type_str = expr_type.get_type();

                // If the target (after peeling array wrappers) is a generic struct, instantiate it.
                let mut core_target = target_type;
                while let Type::Array(inner) = core_target {
                    core_target = inner;
                }
                if let Some((base_name, generic_args)) = Self::resolve_struct_parts(core_target) {
                    self.ensure_struct_instantiated(
                        &base_name,
                        &generic_args,
                        &empty_span(),
                        diagnostics,
                    );
                }

                if target_type_str == expr_type_str ||
                   (is_numeric_primitive(&target_type_str) && is_numeric_primitive(&expr_type_str)) ||
                   // `char` is a code point: allow lossless conversion to/from `int`.
                   (target_type_str == "char" && expr_type_str == "int") ||
                   (target_type_str == "int" && expr_type_str == "char")
                {
                    Ok(target_type.clone())
                } else if target_type_str == "object" || expr_type_str == "object" {
                    // Boxing (`T as object`) and unboxing (`object as T`) are always permitted;
                    // an unbox to the wrong primitive traps at runtime.
                    Ok(target_type.clone())
                } else if expr_type_str == "int"
                    && (self.struct_table.get_struct(&target_type_str).is_some()
                        || target_type_str.ends_with("[]")
                        || target_type_str.ends_with("?"))
                {
                    // Allow casting int to pointer types (for null pointers)
                    Ok(target_type.clone())
                } else {
                    diagnostics.report_error(
                        format!("Cannot cast from {} to {}", expr_type_str, target_type_str),
                        target_type.get_span().or_else(|| expr.position()),
                    );
                    Ok(target_type.clone())
                }
            }
            ExpressionNode::MethodCall(obj, method, generic_args, params) => {
                let ctx = super::AnalyzerContext {
                    parent_function,
                    symbol_table,
                };
                self.analyze_method_call(obj, method, generic_args, params, &ctx, diagnostics)
            }
            ExpressionNode::Await(inner) => {
                let fut =
                    self.analyze_expression(inner, parent_function, symbol_table, diagnostics)?;
                match Self::future_inner_type(&fut) {
                    Some(t) => Ok(t),
                    None => {
                        diagnostics.report_error(
                            format!("'await' expects a Future value, got {}", fut.get_type()),
                            inner.position(),
                        );
                        Ok(Type::Void)
                    }
                }
            }
        }
    }
    pub(super) fn analyze_binary_expression(
        &mut self,
        left: &ExpressionNode<'a>,
        opr: &SyntaxToken,
        right: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        let left_value =
            self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
        let right_value =
            self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;

        // Null-coalescing `a ?? b`: `a` should be nullable; the result is the unwrapped element
        // type, and `b` must be assignable to it (or itself nullable of the same element type).
        if opr.kind == TokenKind::QuestionQuestionToken {
            let result_type = match &left_value {
                Type::Nullable(inner) => (**inner).clone(),
                other => other.clone(),
            };
            let right_unwrapped = match &right_value {
                Type::Nullable(inner) => (**inner).clone(),
                other => other.clone(),
            };
            self.compare_data_type(&result_type, &right_unwrapped, &opr.position, diagnostics)?;
            return Ok(result_type);
        }

        self.compare_data_type(&left_value, &right_value, &opr.position, diagnostics)?;
        match (&left_value, &opr.kind) {
            (Type::String(_), TokenKind::PlusToken) => {}
            // Reference (identity) equality is allowed on strings and objects.
            (Type::String(_), TokenKind::EqualEqualToken)
            | (Type::String(_), TokenKind::NotEqualToken) => {}
            (Type::String(_), _) => {
                diagnostics.report_error(
                    format!("Cannot perform operation {} on string", opr.text),
                    Some(opr.position),
                );
            }
            (_, _) => {}
        };

        match opr.kind {
            TokenKind::EqualEqualToken
            | TokenKind::NotEqualToken
            | TokenKind::GreaterThanToken
            | TokenKind::GreaterThanEqualToken
            | TokenKind::SmallerThanToken
            | TokenKind::SmallerThanEqualToken
            | TokenKind::AmpersandAmpersandToken
            | TokenKind::PipePipeToken => Ok(Type::Boolean(opr.clone())),
            _ => Ok(left_value),
        }
    }
    pub(super) fn compare_data_type(
        &mut self,
        left: &Type,
        right: &Type,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        if left.get_type() == right.get_type() {
            return Ok(());
        }
        if self.enum_int_compatible(&left.get_type(), &right.get_type()) {
            return Ok(());
        }

        // Any value may be assigned (boxed) into an `object` target; the reverse requires a
        // cast and is rejected here.
        if left.get_type() == "object" {
            return Ok(());
        }

        // A nullable `T?` accepts another `T?`, a plain `T`, or the `null` literal (`void?`).
        if let Type::Nullable(inner) = left {
            if let Type::Nullable(inner_right) = right {
                if inner.get_type() == inner_right.get_type() {
                    return Ok(());
                }
            } else if inner.get_type() == right.get_type() {
                return Ok(());
            }
            if right.get_type() == "void?" {
                return Ok(());
            }
        }

        // Any reference type (or nullable) can be compared against the `null` literal.
        if (left.get_type().ends_with("?") || self.is_reference_type(&left.get_type()))
            && right.get_type() == "void?"
        {
            return Ok(());
        }
        if (right.get_type().ends_with("?") || self.is_reference_type(&right.get_type()))
            && left.get_type() == "void?"
        {
            return Ok(());
        }

        diagnostics.report_error(
            format!(
                "cannot convert from {} to {} at {}",
                left.get_type(),
                right.get_type(),
                position.get_point_str()
            ),
            Some(*position),
        );
        Ok(())
    }

    pub fn is_reference_type(&self, type_name: &str) -> bool {
        if self.struct_table.is_reference_type(type_name) {
            return true;
        }
        // A not-yet-instantiated generic struct instance (e.g. `Box_int`) is also a reference type.
        let base_name = strip_nullable(type_name);
        self.demangle_generic_struct(base_name).is_some()
    }
    pub(super) fn analyze_identifier(
        &mut self,
        id: &SyntaxToken,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, ()> {
        let r = match (*symbol_table).as_ref().borrow().get_symbol(id) {
            Ok(t) => t,
            Err(e) => {
                // A bare identifier that names a top-level function is a first-class function value.
                if let Ok(sig) = self.function_table.get_function(&id.text) {
                    let params = sig
                        .parameters
                        .iter()
                        .map(|p| Self::type_from_name(p))
                        .collect();
                    let ret = sig.return_type.clone().unwrap_or(Type::Void);
                    return Ok(Type::Function(params, Box::new(ret)));
                }
                diagnostics.report_error(e.to_string(), Some(id.position));
                Type::Void
            }
        };
        Ok(r)
    }

    /// Reconstructs a `Type` from its canonical type-name string (as stored in function-table
    /// signatures), e.g. "int", "string", "Node", "int[]". Falls back to `void` if unparseable.
    pub(super) fn type_from_name(name: &str) -> Type {
        let token = synthetic_token(TokenKind::IdentifierToken, name);
        Type::from_token(token).unwrap_or(Type::Void)
    }
}
