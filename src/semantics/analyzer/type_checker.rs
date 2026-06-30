use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::semantics::errors::SemanticError;
use crate::semantics::function_control_flow::FunctionControlGraph;
use crate::semantics::symbol_table::SymbolTable;
use crate::syntax::nodes::{FunctionNode, StatementNode};
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_function(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Rc<RefCell<SymbolTable>>, SemanticError> {
        let param_table = Rc::new(RefCell::new(
            self.add_function_param_table(function, diagnostics)?,
        ));
        self.with_async_flag(function.is_async, |s| {
            s.analyze_body(
                function.body,
                function,
                Some(&param_table),
                false,
                diagnostics,
            )?;
            // Enforce the v1 `await` placement rules (only in async functions, only at statement
            // position) and that non-async functions contain no `await` at all.
            s.check_await_positions(function, diagnostics);
            Ok(())
        })?;
        // check return
        let mut graph = FunctionControlGraph::new(function);
        if let Err(e) = graph.build() {
            diagnostics.report_error(e.to_string(), Some(function.name.position));
        }
        Ok(param_table.clone())
    }

    pub(super) fn add_function_param_table(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<SymbolTable, SemanticError> {
        // Parent the parameter table off the module-global scope so function bodies resolve
        // top-level variables (and their `const`-ness) through ordinary lexical lookup.
        let mut param_table = SymbolTable::new(Some(self.global_symbol_table.clone()));
        for param in function.parameters.iter() {
            self.check_reserved_name(&param.name, "parameter", diagnostics);
            if let Err(e) = param_table.add_symbol(param.name.text.clone(), param.type_.clone()) {
                diagnostics.report_error(e.to_string(), Some(param.name.position));
            }
        }
        Ok(param_table)
    }

    pub(super) fn analyze_body(
        &mut self,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        parent_table: Option<&Rc<RefCell<SymbolTable>>>,
        has_parent_loop: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let parent_scope = match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope.clone())));
        if let Some(parent_table) = parent_scope {
            (*parent_table).borrow_mut().add_child(symbol_table.clone());
        }
        for statement in body.iter() {
            let clone = &symbol_table.clone();
            // Recover at the statement boundary: a short-circuited statement leaves its diagnostic
            // in the bag, and we move on to the next sibling so every independent error in the
            // block is still reported (matching the previous poison-and-continue behavior).
            let _ = self.analyze_statement(
                statement,
                parent_function,
                clone,
                has_parent_loop,
                diagnostics,
            );
        }
        Ok(())
    }
    pub(super) fn analyze_statement(
        &mut self,
        statement: &StatementNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let ctx = super::AnalyzerContext {
            parent_function,
            symbol_table,
        };
        match statement {
            StatementNode::Declaration(left, type_annotation, right, is_const) => self
                .analyze_declaration(left, type_annotation, right, *is_const, &ctx, diagnostics)?,
            StatementNode::Assignment(left, right) => {
                self.analyze_assignment(left, right, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::IndexAssignment(left, index, right) => self.analyze_index_assignment(
                left,
                index,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?,
            StatementNode::MemberAssignment(obj, member, right) => self.analyze_member_assignment(
                obj,
                member,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?,
            StatementNode::IfElse(..) => {
                self.analyze_if_else(statement, &ctx, has_parent_while, diagnostics)?
            }
            StatementNode::Return(expression) => {
                self.analyze_return(expression, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::While(condition, body) => {
                self.analyze_while(condition, body, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::DoWhile(body, condition) => {
                self.analyze_while(condition, body, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::For(init, condition, increment, body) => {
                self.analyze_for(init, condition, increment, body, &ctx, diagnostics)?
            }
            StatementNode::ForEach(..) => self.analyze_foreach(statement, &ctx, diagnostics)?,
            StatementNode::Switch(subject, cases, default_body) => self.analyze_switch(
                subject,
                cases,
                default_body,
                &ctx,
                has_parent_while,
                diagnostics,
            )?,
            StatementNode::Labeled(label, inner) => {
                self.loop_labels.push(label.clone());
                let result = self.analyze_statement(
                    inner,
                    parent_function,
                    symbol_table,
                    has_parent_while,
                    diagnostics,
                );
                self.loop_labels.pop();
                result?;
            }
            StatementNode::Break(label) => {
                self.analyze_break(label, parent_function, has_parent_while, diagnostics)?
            }
            StatementNode::Continue(label) => {
                self.analyze_continue(label, parent_function, has_parent_while, diagnostics)?
            }
            StatementNode::FunctionInvocation(name, generic_args, params) => {
                let _ = self.analyze_function_call(
                    name,
                    generic_args,
                    params,
                    parent_function,
                    symbol_table,
                    diagnostics,
                );
            }
            StatementNode::ExpressionStatement(expr) => {
                // A statement-position `match` allows block arms and yields no value.
                if let crate::syntax::nodes::ExpressionNode::Match(subject, arms) = expr {
                    let _ = self.analyze_match(
                        subject,
                        arms,
                        parent_function,
                        symbol_table,
                        false,
                        diagnostics,
                    );
                } else {
                    let _ =
                        self.analyze_expression(expr, parent_function, symbol_table, diagnostics);
                }
            }
            StatementNode::MethodInvocation(obj, method, generic_args, params) => {
                let _ =
                    self.analyze_method_call(obj, method, generic_args, params, &ctx, diagnostics);
            }
            StatementNode::AwaitStmt(future_expr) => {
                let fut = self
                    .analyze_expression(future_expr, parent_function, symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                if Self::future_inner_type(&fut).is_none() {
                    diagnostics.report_error(
                        format!("'await' expects a Future value, got {}", fut.get_type()),
                        future_expr.position(),
                    );
                }
            }
        };
        Ok(())
    }
}
