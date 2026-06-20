use std::io::Error;
use crate::lang::code_analysis::syntax::nodes::{StatementNode, FunctionNode, ExpressionNode};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds the body of a function
    pub fn build_body(&self, statements: &[StatementNode<'a>], function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        for i in statements.iter() {
            self.build_statement(i, function, writer)?;
        }
        Ok(())
    }

    /// Builds a single statement
    pub fn build_statement(&self, statement: &StatementNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        match statement {
            StatementNode::Declaration(left, expression) => self.build_declaration(left, function, expression, writer)?,
            StatementNode::Assignment(left, expression) => self.build_assignment(left, expression, function, writer)?,
            StatementNode::Return(r) => self.build_return(r, function, writer)?,
            StatementNode::While(c, b) => self.build_while(c, b, function, writer)?,
            StatementNode::For(init, cond, inc, body) => self.build_for(init, cond, inc, body, function, writer)?,
            StatementNode::Break => self.build_break(writer)?,
            StatementNode::Continue => self.build_continue(writer)?,
            StatementNode::IfElse(c, b, else_if, else_b) => self.build_if_else(c, b, else_if, else_b, function, writer)?,
            StatementNode::FunctionInvocation(n, p) => self.build_function_invocation(&n.text.clone(), p, function, writer)?,
        }
        Ok(())
    }

    /// Builds a variable declaration
    pub fn build_declaration(&self, left: &SyntaxToken, function: &FunctionNode<'a>, expression: &ExpressionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        self.build_expression(expression, &self.table_read_type(&left.text, function), function, writer)?;
        writer.write_line(&format!("local.set ${}", left.text));
        Ok(())
    }

    /// Builds a variable assignment
    pub fn build_assignment(&self, left: &SyntaxToken, expression: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        self.build_expression(expression, &self.table_read_type(&left.text, function), function, writer)?;
        writer.write_line(&format!("local.set ${}", left.text));
        Ok(())
    }

    /// Builds a return statement
    pub fn build_return(&self, expression: &Option<ExpressionNode<'a>>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(expr) = expression {
            let return_type = function.return_type.as_ref().unwrap();
            self.build_expression(expr, &return_type.get_type(), function, writer)?;
        }
        writer.write_line("return");
        Ok(())
    }

    /// Builds a while loop
    pub fn build_while(&self, condition: &ExpressionNode<'a>, body: &[StatementNode<'a>], function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("(block");
        writer.indent();
        writer.write_line("(loop");
        writer.indent();
        self.build_expression(condition, &"int".to_string(), function, writer)?;
        writer.write_line("i32.const 0");
        writer.write_line("i32.eq");
        writer.write_line("br_if 1");
        self.build_body(body, function, writer)?;
        writer.write_line("br 0");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds a for loop
    pub fn build_for(&self, init: &Option<&'a StatementNode<'a>>, condition: &Option<ExpressionNode<'a>>, increment: &Option<&'a StatementNode<'a>>, body: &[StatementNode<'a>], function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(init_stmt) = init {
            self.build_statement(init_stmt, function, writer)?;
        }
        writer.write_line("(block");
        writer.indent();
        writer.write_line("(loop");
        writer.indent();
        
        if let Some(cond_expr) = condition {
            self.build_expression(cond_expr, &"int".to_string(), function, writer)?;
            writer.write_line("i32.const 0");
            writer.write_line("i32.eq");
            writer.write_line("br_if 1");
        }
        
        self.build_body(body, function, writer)?;
        
        if let Some(inc_stmt) = increment {
            self.build_statement(inc_stmt, function, writer)?;
        }
        
        writer.write_line("br 0");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds a break statement
    pub fn build_break(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("br 1");
        Ok(())
    }

    /// Builds a continue statement
    pub fn build_continue(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("br 0");
        Ok(())
    }

    /// Builds an if-else statement
    pub fn build_if_else(&self, condition: &ExpressionNode<'a>, body: &'a [StatementNode<'a>], else_if: &Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>, else_body: &Option<&'a [StatementNode<'a>]>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let mut arr: Vec<(Option<&ExpressionNode<'a>>, &'a [StatementNode<'a>])> = Vec::new();
        arr.push((Some(condition), body));
        for i in else_if.iter() {
            arr.push((Some(&i.0), i.1));
        }
        if let Some(eb) = else_body {
            arr.push((None, eb));
        }
        self.build_if_else_parts(&arr, function, 0, writer)?;
        Ok(())
    }

    /// Recursively builds the parts of an if-else chain
    pub fn build_if_else_parts(&self, parts: &Vec<(Option<&ExpressionNode<'a>>, &'a [StatementNode<'a>])>, function: &FunctionNode<'a>, index: usize, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if index == parts.len() {
            return Ok(());
        }
        let cur = &parts[index];
        if cur.0.is_none() && index == parts.len() - 1 {
            self.build_body(cur.1, function, writer)?;
        } else {
            self.build_expression(cur.0.unwrap(), &"int".to_string(), function, writer)?;
            writer.write_line("(if");
            writer.indent();
            writer.write_line("(then");
            writer.indent();
            self.build_body(cur.1, function, writer)?;
            writer.unindent();
            writer.write_line(")");
            if index + 1 < parts.len() {
                writer.write_line("(else");
                writer.indent();
                self.build_if_else_parts(parts, function, index + 1, writer)?;
                writer.unindent();
                writer.write_line(")");
            }
            writer.unindent();
            writer.write_line(")");
        }
        Ok(())
    }
}
