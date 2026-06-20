use crate::lang::code_analysis::syntax::nodes::{ProgramNode, StatementNode, ExpressionNode, Type};
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Collects all string literals from the program to place them in the data segment
    pub fn collect_strings_from_program(&mut self, program: &ProgramNode<'a>) {
        for func in &program.functions {
            self.collect_strings_from_body(func.body);
        }
    }

    /// Collects all string literals from a body of statements
    pub fn collect_strings_from_body(&mut self, body: &[StatementNode<'a>]) {
        for stmt in body {
            match stmt {
                StatementNode::Declaration(_, _, expr) | StatementNode::Assignment(_, expr) => {
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::IndexAssignment(_, index, expr) => {
                    self.collect_strings_from_expr(index);
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::IfElse(cond, if_body, else_ifs, else_body) => {
                    self.collect_strings_from_expr(cond);
                    self.collect_strings_from_body(if_body);
                    for (elif_cond, elif_body) in else_ifs {
                        self.collect_strings_from_expr(elif_cond);
                        self.collect_strings_from_body(elif_body);
                    }
                    if let Some(eb) = else_body {
                        self.collect_strings_from_body(eb);
                    }
                }
                StatementNode::While(cond, body) => {
                    self.collect_strings_from_expr(cond);
                    self.collect_strings_from_body(body);
                }
                StatementNode::For(init, cond, inc, body) => {
                    if let Some(init_stmt) = init {
                        self.collect_strings_from_body(std::slice::from_ref(*init_stmt));
                    }
                    if let Some(cond_expr) = cond {
                        self.collect_strings_from_expr(cond_expr);
                    }
                    if let Some(inc_stmt) = inc {
                        self.collect_strings_from_body(std::slice::from_ref(*inc_stmt));
                    }
                    self.collect_strings_from_body(body);
                }
                StatementNode::Return(Some(expr)) => {
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::FunctionInvocation(_, _, params) => {
                    for param in params {
                        self.collect_strings_from_expr(param);
                    }
                }
                _ => {}
            }
        }
    }

    /// Collects all string literals from an expression
    pub fn collect_strings_from_expr(&mut self, expr: &ExpressionNode<'a>) {
        match expr {
            ExpressionNode::Literal(Type::String(token)) => {
                let s = token.text.clone();
                if !self.strings.contains_key(&s) {
                    self.strings.insert(s.clone(), self.next_string_offset);
                    self.next_string_offset += s.len() + 1 + 8; // +1 for null terminator, +8 for block header
                }
            }
            ExpressionNode::Binary(left, _, right) => {
                self.collect_strings_from_expr(left);
                self.collect_strings_from_expr(right);
            }
            ExpressionNode::Unary(_, right) => {
                self.collect_strings_from_expr(right);
            }
            ExpressionNode::Parenthesized(inner) => {
                self.collect_strings_from_expr(inner);
            }
            ExpressionNode::FunctionCall(_, _, params) => {
                for param in params {
                    self.collect_strings_from_expr(param);
                }
            }
            ExpressionNode::ArrayLiteral(elements) => {
                for element in elements {
                    self.collect_strings_from_expr(element);
                }
            }
            ExpressionNode::IndexAccess(array_expr, index_expr) => {
                self.collect_strings_from_expr(array_expr);
                self.collect_strings_from_expr(index_expr);
            }
            _ => {}
        }
    }
}
