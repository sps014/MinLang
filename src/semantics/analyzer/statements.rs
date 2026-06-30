//! Analysis of statements and control flow: loops, switches, declarations, assignments,
//! `if`/`else`, `return`, and the `break`/`continue` placement checks.

use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::intrinsics;
use crate::semantics::symbol_table::SymbolTable;
use crate::syntax::nodes::types::mangle_generic;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, StatementNode, Type};
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_break(
        &mut self,
        label: &Option<String>,
        parent_function: &FunctionNode<'a>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        if !has_parent_while {
            diagnostics.report_error(
                format!(
                    "Break statement is not in a loop in function {}",
                    parent_function.name.text
                ),
                Some(parent_function.name.position),
            );
        }
        if let Some(name) = label {
            if !self.loop_labels.contains(name) {
                diagnostics.report_error(
                    format!("Break targets unknown loop label '{}'", name),
                    Some(parent_function.name.position),
                );
            }
        }
        Ok(())
    }
    pub(super) fn analyze_continue(
        &mut self,
        label: &Option<String>,
        parent_function: &FunctionNode<'a>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        if !has_parent_while {
            diagnostics.report_error(
                format!(
                    "Continue statement is not in a loop in function {}",
                    parent_function.name.text
                ),
                Some(parent_function.name.position),
            );
        }
        if let Some(name) = label {
            if !self.loop_labels.contains(name) {
                diagnostics.report_error(
                    format!("Continue targets unknown loop label '{}'", name),
                    Some(parent_function.name.position),
                );
            }
        }
        Ok(())
    }
    pub(super) fn analyze_foreach(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let StatementNode::ForEach(element, iterable, index_name, array_name, body) = statement
        else {
            unreachable!()
        };
        let iterable_type =
            self.analyze_expression(iterable, ctx.parent_function, ctx.symbol_table, diagnostics)?;
        let element_type = match &iterable_type {
            Type::Array(inner) => (**inner).clone(),
            _ => {
                diagnostics.report_error(
                    format!(
                        "for-each can only iterate over arrays, got {}",
                        iterable_type.get_type()
                    ),
                    iterable.position(),
                );
                Type::Void
            }
        };

        // Register the synthetic loop locals plus the user's element binding in a dedicated scope.
        let foreach_scope = Rc::new(RefCell::new(SymbolTable::new(Some(
            ctx.symbol_table.clone(),
        ))));
        (*ctx.symbol_table)
            .borrow_mut()
            .add_child(foreach_scope.clone());
        {
            let mut scope = (*foreach_scope).borrow_mut();
            let _ = scope.add_symbol(array_name.to_string(), iterable_type.clone());
            let _ = scope.add_symbol(
                index_name.to_string(),
                Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int")),
            );
            if let Err(e) = scope.add_symbol(element.text.clone(), element_type) {
                diagnostics.report_error(e.to_string(), Some(element.position));
            }
        }
        self.analyze_body(
            body,
            ctx.parent_function,
            Some(&foreach_scope),
            true,
            diagnostics,
        )?;
        Ok(())
    }
    pub(super) fn analyze_switch(
        &mut self,
        subject: &ExpressionNode<'a>,
        cases: &Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
        default_body: &Option<&'a [StatementNode<'a>]>,
        ctx: &super::AnalyzerContext<'a, '_>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let subject_type =
            self.analyze_expression(subject, ctx.parent_function, ctx.symbol_table, diagnostics)?;
        let subject_name = subject_type.get_type();
        let subject_is_enum = self.enum_table.contains_key(&subject_name);
        if !matches!(subject_name.as_str(), "int" | "string" | "bool") && !subject_is_enum {
            diagnostics.report_error(
                format!(
                    "switch subject must be int, string, bool, or an enum, got {}",
                    subject_name
                ),
                subject.position(),
            );
        }

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (labels, body) in cases.iter() {
            for label in labels.iter() {
                // Labels must be compile-time constants: a literal, or (for enum switches) an
                // enum member access like `Color.Red`.
                let is_enum_label = matches!(label, ExpressionNode::MemberAccess(_, _));
                if !matches!(label, ExpressionNode::Literal(_)) && !is_enum_label {
                    diagnostics.report_error(
                        "switch case labels must be constant literals or enum members".to_string(),
                        label.position(),
                    );
                }
                let label_type = self.analyze_expression(
                    label,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )?;
                self.compare_data_type(&subject_type, &label_type, &empty_span(), diagnostics)?;

                let key = match label {
                    ExpressionNode::Literal(lit) =>
                    {
                        #[allow(clippy::collapsible_match)]
                        match lit {
                            Type::Integer(t)
                            | Type::Float(t)
                            | Type::Double(t)
                            | Type::String(t)
                            | Type::Boolean(t) => Some(t.text.clone()),
                            _ => None,
                        }
                    }
                    ExpressionNode::MemberAccess(_, m) => Some(m.text.clone()),
                    _ => None,
                };
                if let Some(k) = key {
                    if !seen.insert(k.clone()) {
                        diagnostics.report_error(
                            format!("duplicate case label '{}' in switch statement", k),
                            label.position(),
                        );
                    }
                }
            }
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
        }

        if let Some(db) = default_body {
            self.analyze_body(
                db,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
        }
        Ok(())
    }
    pub(super) fn analyze_while(
        &mut self,
        condition: &ExpressionNode<'a>,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let cond_type =
            self.analyze_expression(condition, parent_function, symbol_table, diagnostics)?;
        if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
            diagnostics.report_error(
                format!("while condition must be bool, got {}", cond_type.get_type()),
                condition.position(),
            );
        }
        self.analyze_body(body, parent_function, Some(symbol_table), true, diagnostics)?;
        Ok(())
    }
    pub(super) fn analyze_for(
        &mut self,
        init: &Option<&'a StatementNode<'a>>,
        condition: &Option<ExpressionNode<'a>>,
        increment: &Option<&'a StatementNode<'a>>,
        body: &[StatementNode<'a>],
        ctx: &super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let for_scope = Rc::new(RefCell::new(SymbolTable::new(Some(
            ctx.symbol_table.clone(),
        ))));
        (*ctx.symbol_table)
            .borrow_mut()
            .add_child(for_scope.clone());

        if let Some(init_stmt) = init {
            self.analyze_statement(
                init_stmt,
                ctx.parent_function,
                &for_scope,
                false,
                diagnostics,
            )?;
        }
        if let Some(cond_expr) = condition {
            let cond_type =
                self.analyze_expression(cond_expr, ctx.parent_function, &for_scope, diagnostics)?;
            if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
                diagnostics.report_error(
                    format!("for condition must be bool, got {}", cond_type.get_type()),
                    cond_expr.position(),
                );
            }
        }
        if let Some(inc_stmt) = increment {
            self.analyze_statement(
                inc_stmt,
                ctx.parent_function,
                &for_scope,
                false,
                diagnostics,
            )?;
        }
        self.analyze_body(
            body,
            ctx.parent_function,
            Some(&for_scope),
            true,
            diagnostics,
        )?;
        Ok(())
    }
    ///return type is returned currently int and float supported
    /// Reports a clear diagnostic when a reserved word (a builtin name or primitive type name) is
    /// used where a user-chosen identifier is expected (`role` is e.g. "variable"/"function").
    pub(super) fn check_reserved_name(
        &self,
        token: &SyntaxToken,
        role: &str,
        diagnostics: &mut DiagnosticBag,
    ) {
        // bare callable, so it is a legal ordinary identifier.
        const RESERVED_TYPE_AND_LITERAL_NAMES: &[&str] = &[
            "int", "float", "double", "string", "bool", "char", "object", "void",
            // C#/.NET-style aliases for the primitives (see `canonical_type_name`).
            "String", "Int32", "Int64", "Single", "Double", "Boolean", "Char", "Object", "Void",
            "true", "false", "null",
        ];
        // The builtin callables are reserved too; sourced from the intrinsic registry so this list
        // never drifts from the set of names the compiler special-cases.
        let is_reserved = RESERVED_TYPE_AND_LITERAL_NAMES.contains(&token.text.as_str())
            || intrinsics::is_object_builtin(&token.text);
        if is_reserved {
            diagnostics.report_error(
                format!(
                    "'{}' is a reserved word and cannot be used as a {} name",
                    token.text, role
                ),
                Some(token.position),
            );
        }
    }

    pub(super) fn analyze_declaration(
        &mut self,
        left: &SyntaxToken,
        type_annotation: &Option<Type>,
        right: &ExpressionNode<'a>,
        is_const: bool,
        ctx: &super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        self.check_reserved_name(left, "variable", diagnostics);
        // Empty array literals carry no element type, so the declaration must supply one via an
        // array-typed annotation (e.g. `let xs: int[] = [];`).
        if let ExpressionNode::ArrayLiteral(elements) = right {
            if elements.is_empty() {
                match type_annotation {
                    Some(t) if t.is_array() => {
                        if let Err(e) = (*ctx.symbol_table)
                            .as_ref()
                            .borrow_mut()
                            .add_symbol(left.text.clone(), t.clone())
                        {
                            diagnostics.report_error(e.to_string(), Some(left.position));
                        }
                        if is_const {
                            (*ctx.symbol_table)
                                .as_ref()
                                .borrow_mut()
                                .mark_const(left.text.clone());
                        }
                        return Ok(());
                    }
                    _ => {
                        diagnostics.report_error(
                            "Empty array literal requires an array type annotation, e.g. `let xs: int[] = [];`".to_string(),
                            Some(left.position),
                        );
                        return Ok(());
                    }
                }
            }
        }
        //return right type. A type annotation is published as the expected type so a generic
        // union's nullary variant (`let o: Option<int> = Option.None;`) can resolve its arguments.
        let saved_expected = self.current_expected_type.take();
        self.current_expected_type = type_annotation.clone();
        let right_type =
            self.analyze_expression(right, ctx.parent_function, ctx.symbol_table, diagnostics)?;
        self.current_expected_type = saved_expected;

        let var_type = if let Some(t) = type_annotation {
            self.compare_data_type(t, &right_type, &left.position, diagnostics)?;
            t.clone()
        } else {
            right_type.clone()
        };

        if let Err(e) = (*ctx.symbol_table)
            .as_ref()
            .borrow_mut()
            .add_symbol(left.text.clone(), var_type)
        {
            diagnostics.report_error(e.to_string(), Some(left.position));
        }
        if is_const {
            (*ctx.symbol_table)
                .as_ref()
                .borrow_mut()
                .mark_const(left.text.clone());
        }
        Ok(())
    }
    pub(super) fn analyze_assignment(
        &mut self,
        left: &SyntaxToken,
        right: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        if (*symbol_table).as_ref().borrow().is_const(&left.text) {
            diagnostics.report_error(
                format!(
                    "Cannot assign to '{}' because it is a const binding",
                    left.text
                ),
                Some(left.position),
            );
        }
        let r = self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        let l = match (*symbol_table).as_ref().borrow().get_symbol(left) {
            Ok(sym) => sym,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(left.position));
                return Ok(());
            }
        };
        self.compare_data_type(&l, &r, &left.position, diagnostics)?;
        Ok(())
    }

    pub(super) fn analyze_index_assignment(
        &mut self,
        arr: &ExpressionNode<'a>,
        index: &ExpressionNode<'a>,
        right: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let array_type =
            self.analyze_expression(arr, parent_function, symbol_table, diagnostics)?;

        let inner_type = match array_type {
            Type::Array(inner) => *inner,
            _ => {
                diagnostics.report_error(
                    format!("Cannot index into non-array type {}", array_type.get_type()),
                    arr.position(),
                );
                return Ok(());
            }
        };

        let index_type =
            self.analyze_expression(index, parent_function, symbol_table, diagnostics)?;
        if !index_type.is_unknown() && index_type.get_type() != "int" {
            diagnostics.report_error(
                format!(
                    "Array index must be of type int, got {}",
                    index_type.get_type()
                ),
                index.position(),
            );
        }

        let right_type =
            self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&inner_type, &right_type, &empty_span(), diagnostics)?;

        Ok(())
    }

    pub(super) fn analyze_member_assignment(
        &mut self,
        obj: &ExpressionNode<'a>,
        member: &SyntaxToken,
        right: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

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
                return Ok(());
            }
        };

        self.ensure_struct_instantiated(&base_name, &generic_args, &member.position, diagnostics);
        let struct_name = mangle_generic(&base_name, &generic_args);

        let (field_type, field_is_public) = {
            let struct_info = match self.struct_table.get_struct(&struct_name) {
                Some(info) => info,
                None => {
                    diagnostics.report_error(
                        format!("Struct '{}' not found", struct_name),
                        Some(member.position),
                    );
                    return Ok(());
                }
            };

            match struct_info.fields.get(&member.text) {
                Some(info) => (info.type_.clone(), info.is_public),
                None => {
                    diagnostics.report_error(
                        format!(
                            "Field '{}' not found in class '{}'",
                            member.text, struct_name
                        ),
                        Some(member.position),
                    );
                    return Ok(());
                }
            }
        };

        // Private fields (the default) may only be written from within the declaring type's own
        // methods; `public` exposes them to outside code.
        if !field_is_public && !self.in_methods_of(parent_function, &base_name) {
            diagnostics.report_error(
                format!("'{}' is private to '{}'", member.text, base_name),
                Some(member.position),
            );
        }

        let right_type =
            self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&field_type, &right_type, &member.position, diagnostics)?;

        Ok(())
    }
    pub(super) fn analyze_if_else(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &super::AnalyzerContext<'a, '_>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let StatementNode::IfElse(condition, if_body, else_if, else_body) = statement else {
            unreachable!()
        };
        // Check for constant expression from `is`
        let mut is_constant_true = false;
        let mut is_constant_false = false;

        if let ExpressionNode::IsExpression(left, right_type) = condition {
            let left_t =
                self.analyze_expression(left, ctx.parent_function, ctx.symbol_table, diagnostics)?;
            // `is` on an `object` is a runtime check; only non-object operands fold to a constant.
            if left_t.get_type() != "object" {
                if left_t.get_type() == right_type.get_type() {
                    is_constant_true = true;
                } else {
                    is_constant_false = true;
                }
            }
        }

        if !is_constant_false {
            //if condition
            let cond_type = self.analyze_expression(
                condition,
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?;
            if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
                diagnostics.report_error(
                    format!("if condition must be bool, got {}", cond_type.get_type()),
                    condition.position(),
                );
            }
            //if body
            self.analyze_body(
                if_body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
        }

        if is_constant_true {
            return Ok(());
        }

        //else if block
        for i in else_if.iter() {
            let mut elif_constant_true = false;
            let mut elif_constant_false = false;
            if let ExpressionNode::IsExpression(left, right_type) = &i.0 {
                let left_t = self.analyze_expression(
                    left,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )?;
                if left_t.get_type() != "object" {
                    if left_t.get_type() == right_type.get_type() {
                        elif_constant_true = true;
                    } else {
                        elif_constant_false = true;
                    }
                }
            }

            if !elif_constant_false {
                let elif_cond_type = self.analyze_expression(
                    &i.0,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )?;
                if !elif_cond_type.is_unknown() && elif_cond_type.get_type() != "bool" {
                    diagnostics.report_error(
                        format!(
                            "else if condition must be bool, got {}",
                            elif_cond_type.get_type()
                        ),
                        i.0.position(),
                    );
                }
                self.analyze_body(
                    i.1,
                    ctx.parent_function,
                    Some(ctx.symbol_table),
                    has_parent_while,
                    diagnostics,
                )?;
            }

            if elif_constant_true {
                return Ok(());
            }
        }

        if let Some(body) = else_body {
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?
        }
        Ok(())
    }
    pub(super) fn analyze_return(
        &mut self,
        expression: &Option<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        match (expression, &parent_function.return_type) {
            (Some(expression), Some(return_type)) => {
                let saved_expected = self.current_expected_type.take();
                self.current_expected_type = Some(return_type.clone());
                let r = self.analyze_expression(
                    expression,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                self.current_expected_type = saved_expected;
                self.compare_data_type(
                    return_type,
                    &r,
                    &parent_function.name.position,
                    diagnostics,
                )?;
            }
            (None, &Some(_)) => {
                diagnostics.report_error(
                    format!(
                        "return type mismatch at  {}",
                        parent_function.name.position.get_point_str()
                    ),
                    Some(parent_function.name.position),
                );
            }
            (Some(_), &None) => {
                diagnostics.report_error(
                    format!(
                        "return type mismatch at {}",
                        parent_function.name.position.get_point_str()
                    ),
                    Some(parent_function.name.position),
                );
            }
            (None, &None) => (),
        };
        Ok(())
    }
}
