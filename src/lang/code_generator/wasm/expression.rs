use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::syntax::nodes::{ExpressionNode, FunctionNode, Type};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds an expression
    pub fn build_expression(&self, expression: &ExpressionNode<'a>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        match expression {
            ExpressionNode::Identifier(identifier) => self.build_identifier(identifier, writer)?,
            ExpressionNode::Unary(opr, expression) => self.build_unary(opr, expression, left_side, function, writer)?,
            ExpressionNode::Binary(left, opr, right) => self.build_binary(left, opr, right, left_side, function, writer)?,
            ExpressionNode::Literal(literal) => self.build_literal(literal, writer)?,
            ExpressionNode::FunctionCall(n, args) => self.build_function_invocation(&n.text.clone(), args, function, writer)?,
            ExpressionNode::Parenthesized(e) => self.build_expression(e, left_side, function, writer)?,
        }
        Ok(())
    }

    /// Builds a literal value
    pub fn build_literal(&self, literal: &Type, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let type_ = match literal {
            Type::Integer(i) => format!("i32.const {}", i.text),
            Type::Float(f) => format!("f32.const {}", f.text),
            Type::Boolean(f) => format!("i32.const {}", if f.text == "true" { 1 } else { 0 }),
            Type::String(s) => {
                let offset = self.strings.get(&s.text).unwrap();
                format!("i32.const {}", offset)
            },
            _ => return Err(Error::new(ErrorKind::Other, format!("unknown literal {:?}", literal)))
        };
        writer.write_line(&type_);
        Ok(())
    }

    /// Builds a binary expression
    pub fn build_binary(&self, left_exp: &ExpressionNode<'a>, opr: &SyntaxToken, right_expr: &ExpressionNode<'a>, left: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        self.build_expression(left_exp, left, function, writer)?;
        self.build_expression(right_expr, left, function, writer)?;

        if left == "string" && opr.kind == TokenKind::PlusToken {
            writer.write_line("call $concat_strings");
            return Ok(());
        }

        let symbol = WasmGenerator::get_wasm_type_from(left.clone())?;
        match opr.kind {
            TokenKind::PlusToken => writer.write_line(&format!("{}.add", symbol)),
            TokenKind::MinusToken => writer.write_line(&format!("{}.sub", symbol)),
            TokenKind::StarToken => writer.write_line(&format!("{}.mul", symbol)),
            TokenKind::EqualEqualToken => writer.write_line(&format!("{}.eq", symbol)),
            TokenKind::NotEqualToken => writer.write_line(&format!("{}.ne", symbol)),
            _ => {}
        };
        
        if symbol == "f32" {
            match opr.kind {
                TokenKind::SlashToken => writer.write_line(&format!("{}.div", symbol)),
                TokenKind::ModulusToken => writer.write_line(&format!("{}.rem", symbol)),
                TokenKind::GreaterThanToken => writer.write_line(&format!("{}.gt", symbol)),
                TokenKind::SmallerThanToken => writer.write_line(&format!("{}.lt", symbol)),
                TokenKind::GreaterThanEqualToken => writer.write_line(&format!("{}.ge", symbol)),
                TokenKind::SmallerThanEqualToken => writer.write_line(&format!("{}.le", symbol)),
                TokenKind::PlusToken | TokenKind::MinusToken | TokenKind::StarToken | TokenKind::EqualEqualToken | TokenKind::NotEqualToken => {},
                _ => return Err(Error::new(ErrorKind::Other, format!("unknown operator {}", opr.text)))
            };
        } else if symbol == "i32" {
            match opr.kind {
                TokenKind::SlashToken => writer.write_line(&format!("{}.div_s", symbol)),
                TokenKind::ModulusToken => writer.write_line(&format!("{}.rem_s", symbol)),
                TokenKind::GreaterThanToken => writer.write_line(&format!("{}.gt_s", symbol)),
                TokenKind::SmallerThanToken => writer.write_line(&format!("{}.lt_s", symbol)),
                TokenKind::GreaterThanEqualToken => writer.write_line(&format!("{}.ge_s", symbol)),
                TokenKind::SmallerThanEqualToken => writer.write_line(&format!("{}.le_s", symbol)),
                TokenKind::AmpersandAmpersandToken | TokenKind::BitWiseAmpersandToken => writer.write_line(&format!("{}.and", symbol)),
                TokenKind::PipePipeToken | TokenKind::BitWisePipeToken => writer.write_line(&format!("{}.or", symbol)),
                TokenKind::PlusToken | TokenKind::MinusToken | TokenKind::StarToken | TokenKind::EqualEqualToken | TokenKind::NotEqualToken => {},
                _ => return Err(Error::new(ErrorKind::Other, format!("unknown operator {}", opr.text)))
            };
        } else {
            return Err(Error::new(ErrorKind::Other, format!("unknown symbol {}", symbol)));
        }

        Ok(())
    }

    /// Builds a unary expression
    pub fn build_unary(&self, opr: &SyntaxToken, expression: &ExpressionNode<'a>, left: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        self.build_expression(expression, left, function, writer)?;
        let symbol = WasmGenerator::get_wasm_type_from(left.clone())?;
        match opr.kind {
            TokenKind::MinusToken => {
                writer.write_line(&format!("{}.const -1", symbol));
                writer.write_line(&format!("{}.mul", symbol));
            },
            TokenKind::BangToken => {
                writer.write_line(&format!("{}.const 0", symbol));
                writer.write_line(&format!("{}.eq", symbol));
            },
            TokenKind::PlusToken => {},
            _ => return Err(Error::new(ErrorKind::Other, format!("wasm does not support unary operator {}", opr.text)))
        };
        Ok(())
    }

    /// Builds an identifier reference
    pub fn build_identifier(&self, identifier: &SyntaxToken, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line(&format!("local.get ${}", identifier.text));
        Ok(())
    }

    /// Builds a function invocation
    pub fn build_function_invocation(&self, name: &String, parameters: &Vec<ExpressionNode<'a>>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        for i in parameters.iter() {
            self.build_expression(i, &"int".to_string(), function, writer)?;
        }
        writer.write("call $");
        writer.write_line(name);
        Ok(())
    }
}
