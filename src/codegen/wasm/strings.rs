use super::WasmGenerator;
use crate::syntax::nodes::{ExpressionNode, ProgramNode, StatementNode, Type};

impl<'a> WasmGenerator<'a> {
    /// Collects all string literals from the program to place them in the data segment
    pub fn collect_strings_from_program(&mut self, program: &ProgramNode<'a>) {
        for func in &program.functions {
            self.collect_strings_from_body(func.body);
        }
        // Struct method bodies live outside `program.functions`, so collect them too.
        for struct_decl in &program.structs {
            for method in &struct_decl.methods {
                self.collect_strings_from_body(method.body);
            }
        }
        // `extend Type { ... }` blocks (including stdlib extensions like `string`/`JsRef`) also
        // hold method bodies with their own string literals.
        for extend_decl in &program.extends {
            for method in &extend_decl.methods {
                self.collect_strings_from_body(method.body);
            }
        }
    }

    /// Collects all string literals from a body of statements
    pub fn collect_strings_from_body(&mut self, body: &[StatementNode<'a>]) {
        for stmt in body {
            match stmt {
                StatementNode::Declaration(_, _, expr, _) | StatementNode::Assignment(_, expr) => {
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::IndexAssignment(arr, index, expr) => {
                    self.collect_strings_from_expr(arr);
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
                StatementNode::DoWhile(body, cond) => {
                    self.collect_strings_from_body(body);
                    self.collect_strings_from_expr(cond);
                }
                StatementNode::Labeled(_, inner) => {
                    self.collect_strings_from_body(std::slice::from_ref(*inner));
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
                StatementNode::MethodInvocation(obj, _, _, params) => {
                    self.collect_strings_from_expr(obj);
                    for param in params {
                        self.collect_strings_from_expr(param);
                    }
                }
                StatementNode::MemberAssignment(obj, _, expr) => {
                    self.collect_strings_from_expr(obj);
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::ForEach(_, iterable, _, _, body) => {
                    self.collect_strings_from_expr(iterable);
                    self.collect_strings_from_body(body);
                }
                StatementNode::Switch(subject, cases, default_body) => {
                    self.collect_strings_from_expr(subject);
                    for (labels, body) in cases {
                        for label in labels {
                            self.collect_strings_from_expr(label);
                        }
                        self.collect_strings_from_body(body);
                    }
                    if let Some(db) = default_body {
                        self.collect_strings_from_body(db);
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
                if !self.ctx.strings.contains_key(&s) {
                    self.ctx
                        .strings
                        .insert(s.clone(), self.ctx.next_string_offset);
                    // +1 for the null terminator, + header for the next block's [size][tag][ref_count].
                    self.ctx.next_string_offset += s.len() + 1 + super::HEAP_HEADER_SIZE;
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
            ExpressionNode::StructInstantiation(_, _, fields) => {
                for (_, expr) in fields {
                    self.collect_strings_from_expr(expr);
                }
            }
            ExpressionNode::MemberAccess(obj, _) => {
                self.collect_strings_from_expr(obj);
            }
            ExpressionNode::Cast(_, expr) => {
                self.collect_strings_from_expr(expr);
            }
            ExpressionNode::IsExpression(expr, _) => {
                self.collect_strings_from_expr(expr);
            }
            ExpressionNode::MethodCall(obj, _, _, params) => {
                self.collect_strings_from_expr(obj);
                for param in params {
                    self.collect_strings_from_expr(param);
                }
            }
            ExpressionNode::Ternary(cond, then_e, else_e) => {
                self.collect_strings_from_expr(cond);
                self.collect_strings_from_expr(then_e);
                self.collect_strings_from_expr(else_e);
            }
            ExpressionNode::Await(inner) => {
                self.collect_strings_from_expr(inner);
            }
            _ => {}
        }
    }
}
