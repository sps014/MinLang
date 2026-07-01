//! Analysis of statements and control flow: loops, switches, declarations, assignments,
//! `if`/`else`, `return`, and the `break`/`continue` placement checks.

use super::*;
use crate::diagnostics::DiagnosticBag;
use crate::hir::{HExpr, HStmt};
use crate::intrinsics;
use crate::semantics::errors::SemanticError;
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
    ) -> Result<(), SemanticError> {
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
        self.hir_break(label.clone());
        Ok(())
    }
    pub(super) fn analyze_continue(
        &mut self,
        label: &Option<String>,
        parent_function: &FunctionNode<'a>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
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
        self.hir_continue(label.clone());
        Ok(())
    }
    pub(super) fn analyze_foreach(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let StatementNode::ForEach(element, iterable, index_name, array_name, body) = statement
        else {
            unreachable!()
        };
        let iterable_type = self
            .analyze_expression(iterable, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let iter_hir = self.hir_take();
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
            if let Err(e) = scope.add_symbol(element.text.clone(), element_type.clone()) {
                diagnostics.report_error(e.to_string(), Some(element.position));
            }
        }
        // Allocate the element slot before the body so body references resolve to it. The synthetic
        // index/array locals are internal to the MIR `Foreach` lowering and get no HIR slot, so a
        // body that reads the index variable will (correctly) fall out of HIR coverage.
        let elem_slot = self.hir_alloc_local(&element.text, &element_type);
        self.hir_open_block();
        self.analyze_body(
            body,
            ctx.parent_function,
            Some(&foreach_scope),
            true,
            diagnostics,
        )?;
        let body_hir = self.hir_close_block();
        self.hir_foreach(elem_slot, iter_hir, body_hir);
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
    ) -> Result<(), SemanticError> {
        let subject_type = self
            .analyze_expression(subject, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let subject_hir = self.hir_take();
        let mut hir_arms: Vec<crate::hir::HArm> = Vec::new();
        // A multi-label case (`case 1, 2, 3:`) becomes one `HArm` per label, all sharing a clone of
        // the case body (each label is a distinct dispatch target hitting the same code).
        let mut hir_ok = true;
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
            let mut label_hirs: Vec<Option<HExpr>> = Vec::new();
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
                let label_type = self
                    .analyze_expression(label, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                label_hirs.push(self.hir_take());
                self.compare_data_type(&subject_type, &label_type, &empty_span(), diagnostics)?;

                let key = match label {
                    ExpressionNode::Literal(
                        Type::Integer(t)
                        | Type::Float(t)
                        | Type::Double(t)
                        | Type::String(t)
                        | Type::Boolean(t),
                    ) => Some(t.text.clone()),
                    ExpressionNode::Literal(_) => None,
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
            self.hir_open_block();
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
            let body_hir = self.hir_close_block();
            // One arm per label; all labels of a case share (a clone of) its body.
            for label_hir in label_hirs {
                match self.hir_const_arm(label_hir, body_hir.clone()) {
                    Some(arm) => hir_arms.push(arm),
                    None => hir_ok = false,
                }
            }
        }

        let default_hir = if let Some(db) = default_body {
            self.hir_open_block();
            self.analyze_body(
                db,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
            self.hir_close_block()
        } else {
            Vec::new()
        };

        self.hir_switch(subject_hir, hir_arms, default_hir, hir_ok);
        Ok(())
    }
    pub(super) fn analyze_while(
        &mut self,
        condition: &ExpressionNode<'a>,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let cond_type = self
            .analyze_expression(condition, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let cond_hir = self.hir_take();
        if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
            diagnostics.report_error(
                format!("while condition must be bool, got {}", cond_type.get_type()),
                condition.position(),
            );
        }
        self.hir_open_block();
        self.analyze_body(body, parent_function, Some(symbol_table), true, diagnostics)?;
        let body_hir = self.hir_close_block();
        self.hir_while(cond_hir, body_hir);
        Ok(())
    }
    pub(super) fn analyze_do_while(
        &mut self,
        condition: &ExpressionNode<'a>,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let cond_type = self
            .analyze_expression(condition, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let cond_hir = self.hir_take();
        if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
            diagnostics.report_error(
                format!("do/while condition must be bool, got {}", cond_type.get_type()),
                condition.position(),
            );
        }
        self.hir_open_block();
        self.analyze_body(body, parent_function, Some(symbol_table), true, diagnostics)?;
        let body_hir = self.hir_close_block();
        self.hir_do_while(cond_hir, body_hir);
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
    ) -> Result<(), SemanticError> {
        let for_scope = Rc::new(RefCell::new(SymbolTable::new(Some(
            ctx.symbol_table.clone(),
        ))));
        (*ctx.symbol_table)
            .borrow_mut()
            .add_child(for_scope.clone());

        self.hir_open_block();
        if let Some(init_stmt) = init {
            self.analyze_statement(
                init_stmt,
                ctx.parent_function,
                &for_scope,
                false,
                diagnostics,
            )?;
        }
        let init_hir = self.hir_close_block();

        let mut cond_hir = None;
        if let Some(cond_expr) = condition {
            let cond_type = self
                .analyze_expression(cond_expr, ctx.parent_function, &for_scope, diagnostics)
                .unwrap_or(Type::Unknown);
            cond_hir = self.hir_take();
            if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
                diagnostics.report_error(
                    format!("for condition must be bool, got {}", cond_type.get_type()),
                    cond_expr.position(),
                );
            }
        }

        self.hir_open_block();
        if let Some(inc_stmt) = increment {
            self.analyze_statement(
                inc_stmt,
                ctx.parent_function,
                &for_scope,
                false,
                diagnostics,
            )?;
        }
        let step_hir = self.hir_close_block();

        self.hir_open_block();
        self.analyze_body(
            body,
            ctx.parent_function,
            Some(&for_scope),
            true,
            diagnostics,
        )?;
        let body_hir = self.hir_close_block();

        self.hir_for(init_hir, cond_hir, step_hir, body_hir);
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
            "int", "float", "double", "string", "bool", "char", "object", "void", "long", "uint",
            "ulong", "byte",
            // C#/.NET-style aliases for the primitives (see `canonical_type_name`).
            "String", "Int32", "Int64", "UInt32", "UInt64", "Byte", "Single", "Double", "Boolean",
            "Char", "Object", "Void", "true", "false", "null",
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
    ) -> Result<(), SemanticError> {
        self.check_reserved_name(left, "variable", diagnostics);
        // Empty array literals carry no element type, so the declaration must supply one via an
        // array-typed annotation (e.g. `let xs: int[] = [];`).
        if let ExpressionNode::ArrayLiteral(elements) = right {
            if elements.is_empty() {
                self.hir_fail();
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
        // Recover at the binding site: even when the initializer short-circuits, fall back to the
        // poison type so the variable is still registered (with its annotated type, if any) and
        // later uses of it don't spuriously report "does not exist".
        let right_type = self
            .analyze_expression(right, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let value = self.hir_take();
        self.current_expected_type = saved_expected;

        let var_type = if let Some(t) = type_annotation {
            self.compare_data_type(t, &right_type, &left.position, diagnostics)?;
            t.clone()
        } else {
            right_type.clone()
        };

        self.hir_declare_local(&left.text, &var_type, value);

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
    ) -> Result<(), SemanticError> {
        if (*symbol_table).as_ref().borrow().is_const(&left.text) {
            diagnostics.report_error(
                format!(
                    "Cannot assign to '{}' because it is a const binding",
                    left.text
                ),
                Some(left.position),
            );
        }
        let r = self
            .analyze_expression(right, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let value = self.hir_take();
        let l = match (*symbol_table).as_ref().borrow().get_symbol(left) {
            Ok(sym) => sym,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(left.position));
                self.hir_fail();
                return Ok(());
            }
        };
        self.compare_data_type(&l, &r, &left.position, diagnostics)?;
        self.hir_assign_local(&left.text, value);
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
    ) -> Result<(), SemanticError> {
        let array_type = self
            .analyze_expression(arr, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let array_hir = self.hir_take();

        let inner_type = match array_type {
            Type::Array(inner) => *inner,
            _ => {
                self.hir_fail();
                diagnostics.report_error(
                    format!("Cannot index into non-array type {}", array_type.get_type()),
                    arr.position(),
                );
                return Ok(());
            }
        };

        let index_type = self
            .analyze_expression(index, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let index_hir = self.hir_take();
        if !index_type.is_unknown() && index_type.get_type() != "int" {
            diagnostics.report_error(
                format!(
                    "Array index must be of type int, got {}",
                    index_type.get_type()
                ),
                index.position(),
            );
        }

        let right_type = self
            .analyze_expression(right, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let value_hir = self.hir_take();
        self.compare_data_type(&inner_type, &right_type, &empty_span(), diagnostics)?;

        self.hir_assign_index(array_hir, index_hir, value_hir);
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
    ) -> Result<(), SemanticError> {
        let obj_type = self
            .analyze_expression(obj, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let obj_hir = self.hir_take();

        let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
            Some(parts) => parts,
            None => {
                self.hir_fail();
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
                    self.hir_fail();
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
                    self.hir_fail();
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

        let right_type = self
            .analyze_expression(right, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let value_hir = self.hir_take();
        self.compare_data_type(&field_type, &right_type, &member.position, diagnostics)?;

        match self.struct_field_index(&struct_name, &member.text) {
            Some(index) => self.hir_assign_field(obj_hir, index, value_hir),
            None => self.hir_fail(),
        }
        Ok(())
    }
    pub(super) fn analyze_if_else(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &super::AnalyzerContext<'a, '_>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let StatementNode::IfElse(condition, if_body, else_if, else_body) = statement else {
            unreachable!()
        };
        // Check for constant expression from `is`
        let mut is_constant_true = false;
        let mut is_constant_false = false;

        // HIR for the whole chain, folded at the end into a single `HStmt::If`. An `is` condition
        // folds to a compile-time constant (below) which has no clean HIR form, so any `is` in the
        // chain drops the function out of HIR coverage.
        let mut hir_primary_cond: Option<HExpr> = None;
        let mut hir_primary_body: Vec<HStmt> = Vec::new();
        let mut hir_elifs: Vec<(Option<HExpr>, Vec<HStmt>)> = Vec::new();
        let mut hir_else: Vec<HStmt> = Vec::new();

        if let ExpressionNode::IsExpression(left, right_type) = condition {
            self.hir_fail();
            let left_t = self
                .analyze_expression(left, ctx.parent_function, ctx.symbol_table, diagnostics)
                .unwrap_or(Type::Unknown);
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
            let cond_type = self
                .analyze_expression(condition, ctx.parent_function, ctx.symbol_table, diagnostics)
                .unwrap_or(Type::Unknown);
            hir_primary_cond = self.hir_take();
            if !cond_type.is_unknown() && cond_type.get_type() != "bool" {
                diagnostics.report_error(
                    format!("if condition must be bool, got {}", cond_type.get_type()),
                    condition.position(),
                );
            }
            //if body
            self.hir_open_block();
            self.analyze_body(
                if_body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
            hir_primary_body = self.hir_close_block();
        }

        if is_constant_true {
            return Ok(());
        }

        //else if block
        for i in else_if.iter() {
            let mut elif_constant_true = false;
            let mut elif_constant_false = false;
            if let ExpressionNode::IsExpression(left, right_type) = &i.0 {
                self.hir_fail();
                let left_t = self
                    .analyze_expression(left, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                if left_t.get_type() != "object" {
                    if left_t.get_type() == right_type.get_type() {
                        elif_constant_true = true;
                    } else {
                        elif_constant_false = true;
                    }
                }
            }

            if !elif_constant_false {
                let elif_cond_type = self
                    .analyze_expression(&i.0, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let elif_cond_hir = self.hir_take();
                if !elif_cond_type.is_unknown() && elif_cond_type.get_type() != "bool" {
                    diagnostics.report_error(
                        format!(
                            "else if condition must be bool, got {}",
                            elif_cond_type.get_type()
                        ),
                        i.0.position(),
                    );
                }
                self.hir_open_block();
                self.analyze_body(
                    i.1,
                    ctx.parent_function,
                    Some(ctx.symbol_table),
                    has_parent_while,
                    diagnostics,
                )?;
                let elif_body = self.hir_close_block();
                hir_elifs.push((elif_cond_hir, elif_body));
            }

            if elif_constant_true {
                return Ok(());
            }
        }

        if let Some(body) = else_body {
            self.hir_open_block();
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
            hir_else = self.hir_close_block();
        }

        self.hir_if_chain((hir_primary_cond, hir_primary_body), hir_elifs, hir_else);
        Ok(())
    }
    pub(super) fn analyze_return(
        &mut self,
        expression: &Option<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        match (expression, &parent_function.return_type) {
            (Some(expression), Some(return_type)) => {
                let saved_expected = self.current_expected_type.take();
                self.current_expected_type = Some(return_type.clone());
                let r = self
                    .analyze_expression(expression, parent_function, symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let value = self.hir_take();
                self.current_expected_type = saved_expected;
                self.compare_data_type(
                    return_type,
                    &r,
                    &parent_function.name.position,
                    diagnostics,
                )?;
                self.hir_return_value(value);
            }
            (None, &Some(_)) => {
                self.hir_fail();
                diagnostics.report_error(
                    format!(
                        "return type mismatch at  {}",
                        parent_function.name.position.get_point_str()
                    ),
                    Some(parent_function.name.position),
                );
            }
            (Some(_), &None) => {
                self.hir_fail();
                diagnostics.report_error(
                    format!(
                        "return type mismatch at {}",
                        parent_function.name.position.get_point_str()
                    ),
                    Some(parent_function.name.position),
                );
            }
            (None, &None) => self.hir_return_void(),
        };
        Ok(())
    }
}
