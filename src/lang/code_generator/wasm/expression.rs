use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::syntax::nodes::{ExpressionNode, FunctionNode, Type};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds an expression
    pub fn build_expression(&mut self, expression: &ExpressionNode<'a>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        match expression {
            ExpressionNode::Identifier(identifier) => self.build_identifier(identifier, writer)?,
            ExpressionNode::ArrayLiteral(elements) => self.build_array_literal(elements, left_side, function, writer)?,
            ExpressionNode::IndexAccess(array_expr, index_expr) => self.build_index_access(array_expr, index_expr, left_side, function, writer)?,
            ExpressionNode::Unary(opr, expression) => self.build_unary(opr, expression, left_side, function, writer)?,
            ExpressionNode::Binary(left, opr, right) => self.build_binary(left, opr, right, left_side, function, writer)?,
            ExpressionNode::Literal(literal) => self.build_literal(literal, writer)?,
            ExpressionNode::FunctionCall(n, args) => self.build_function_invocation(&n.text.clone(), args, function, writer)?,
            ExpressionNode::Parenthesized(e) => self.build_expression(e, left_side, function, writer)?,
        }
        Ok(())
    }

    /// Builds a literal value
    pub fn build_literal(&mut self, literal: &Type, writer: &mut IndentedTextWriter) -> Result<(), Error> {
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
    pub fn build_binary(&mut self, left_exp: &ExpressionNode<'a>, opr: &SyntaxToken, right_expr: &ExpressionNode<'a>, left: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
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
    pub fn build_unary(&mut self, opr: &SyntaxToken, expression: &ExpressionNode<'a>, left: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
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
    pub fn build_identifier(&mut self, identifier: &SyntaxToken, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line(&format!("local.get ${}", identifier.text));
        Ok(())
    }

    /// Builds a function invocation
    pub fn build_function_invocation(&mut self, name: &String, parameters: &Vec<ExpressionNode<'a>>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let func_info = self.function_table.get_function(name)?;
        
        for (i, expr) in parameters.iter().enumerate() {
            let param_type = if i < func_info.parameters.len() {
                func_info.parameters[i].clone()
            } else {
                "int".to_string() // Fallback if parameter count mismatch (should be caught by semantic analysis)
            };
            self.build_expression(expr, &param_type, function, writer)?;
        }
        writer.write("call $");
        writer.write_line(name);
        Ok(())
    }

    /// Builds an array literal
    pub fn build_array_literal(&mut self, elements: &Vec<ExpressionNode<'a>>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let len = elements.len();
        let total_size = 4 + (len * 4); // 4 bytes for length + 4 bytes per element
        
        let inner_type_str = if left_side.ends_with("[]") {
            left_side[..left_side.len() - 2].to_string()
        } else {
            "int".to_string() // Fallback, shouldn't happen if semantic analysis is correct
        };
        let wasm_type = WasmGenerator::get_wasm_type_from(inner_type_str.clone())?;

        // 1. Get current heap ptr (this will be our array ptr)
        writer.write_line("global.get $heap_ptr");
        
        // 2. Store the length at ptr + 0
        writer.write_line("global.get $heap_ptr"); // ptr for store
        writer.write_line(&format!("i32.const {}", len));
        writer.write_line("i32.store");
        
        // 3. Evaluate and store each element
        for (i, expr) in elements.iter().enumerate() {
            let offset = 4 + (i * 4);
            writer.write_line("global.get $heap_ptr"); // ptr
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add"); // ptr + offset
            
            self.build_expression(expr, &inner_type_str, function, writer)?;
            
            if wasm_type == "f32" {
                writer.write_line("f32.store");
            } else {
                writer.write_line("i32.store");
            }
        }
        
        // 4. Bump the heap pointer
        writer.write_line("global.get $heap_ptr");
        writer.write_line(&format!("i32.const {}", total_size));
        writer.write_line("i32.add");
        writer.write_line("global.set $heap_ptr");
        
        // The original ptr is still on the stack from step 1, which is exactly what we want to return
        Ok(())
    }

    /// Builds an array index access
    pub fn build_index_access(&mut self, array_expr: &ExpressionNode<'a>, index_expr: &ExpressionNode<'a>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Here left_side is the expected type of the expression, which is the inner type of the array
        let wasm_type = WasmGenerator::get_wasm_type_from(left_side.clone())?;
        
        // Calculate the memory address: ptr + 4 + (index * 4)
        // Note: We pass a dummy type "int[]" to build_expression for the array ptr because we just need an i32 back
        self.build_expression(array_expr, &"int[]".to_string(), function, writer)?; // ptr
        writer.write_line("i32.const 4");
        writer.write_line("i32.add"); // ptr + 4
        
        self.build_expression(index_expr, &"int".to_string(), function, writer)?; // index
        writer.write_line("i32.const 4");
        writer.write_line("i32.mul"); // index * 4
        
        writer.write_line("i32.add"); // ptr + 4 + (index * 4)
        
        // Load the value
        if wasm_type == "f32" {
            writer.write_line("f32.load");
        } else {
            writer.write_line("i32.load");
        }
        
        Ok(())
    }
}
