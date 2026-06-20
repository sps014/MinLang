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
            ExpressionNode::FunctionCall(n, generic_args, args) => {
                let mut function_name = n.text.clone();
                // If it's a generic call, mangle the name
                if let Some(generics) = generic_args {
                    if !generics.is_empty() {
                        let type_str = generics[0].get_type();
                        function_name = format!("{}_{}", function_name, type_str);
                    }
                } else if self.function_table.get_function(&function_name).is_err() {
                    // Try to infer generic type from first argument if not explicit
                    if !args.is_empty() {
                        if let Ok(inferred_type) = self.infer_expression_type(&args[0], function) {
                            let mangled = format!("{}_{}", function_name, inferred_type);
                            if self.function_table.get_function(&mangled).is_ok() {
                                function_name = mangled;
                            }
                        }
                    }
                }
                self.build_function_invocation(&function_name, args, function, writer)?
            },
            ExpressionNode::Parenthesized(e) => self.build_expression(e, left_side, function, writer)?,
            ExpressionNode::Cast(target_type, expr) => self.build_cast(target_type, expr, left_side, function, writer)?,
            ExpressionNode::StructInstantiation(name, generic_args, fields) => self.build_struct_instantiation(name, generic_args, fields, left_side, function, writer)?,
            ExpressionNode::MemberAccess(obj, member) => self.build_member_access(obj, member, left_side, function, writer)?,
            ExpressionNode::IsExpression(left, right_type) => {
                let left_type = self.infer_expression_type(left, function)?;
                if left_type == right_type.get_type() {
                    writer.write_line("i32.const 1");
                } else {
                    writer.write_line("i32.const 0");
                }
            },
            ExpressionNode::MethodCall(obj, method, generic_args, params) => self.build_method_call(obj, method, generic_args, params, left_side, function, writer)?,
        }
        Ok(())
    }

    /// Builds a type cast expression
    pub fn build_cast(&mut self, target_type: &Type, expr: &ExpressionNode<'a>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let target_str = target_type.get_type();
        let source_str = self.infer_expression_type(expr, function)?;
        
        self.build_expression(expr, &source_str, function, writer)?;
        
        if target_str == "float" && source_str == "int" {
            writer.write_line("f32.convert_i32_s");
        } else if target_str == "int" && source_str == "float" {
            writer.write_line("i32.trunc_f32_s");
        }
        // If they are the same type, or unsupported cast, do nothing (analyzer already validated it)
        
        Ok(())
    }

    /// Builds a struct instantiation
    pub fn build_struct_instantiation(&mut self, name: &SyntaxToken, generic_args: &Option<Vec<Type>>, fields: &Vec<(SyntaxToken, ExpressionNode<'a>)>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let mut struct_name = name.text.clone();
        if let Some(args) = generic_args {
            if !args.is_empty() {
                struct_name = format!("{}_{}", struct_name, args[0].get_type());
            }
        }
        let struct_info = self.struct_table.get_struct(&struct_name).unwrap().clone();
        
        // 1. Allocate memory using $malloc
        writer.write_line(&format!("i32.const {}", struct_info.size));
        writer.write_line("call $malloc");
        writer.write_line("local.set $scratch_ptr");
        
        // 2. Evaluate and store each field
        for (field_name, expr) in fields.iter() {
            let field_info = struct_info.fields.get(&field_name.text).unwrap();
            let offset = field_info.offset;
            let wasm_type = WasmGenerator::get_wasm_type_from(field_info.type_.get_type())?;
            
            writer.write_line("local.get $scratch_ptr"); // ptr
            if offset > 0 {
                writer.write_line(&format!("i32.const {}", offset));
                writer.write_line("i32.add"); // ptr + offset
            }
            
            self.build_expression(expr, &field_info.type_.get_type(), function, writer)?;
            
            if field_info.type_.get_type() == "bool" {
                writer.write_line("i32.store8");
            } else if wasm_type == "f64" {
                writer.write_line("f64.store");
            } else if wasm_type == "f32" {
                writer.write_line("f32.store");
            } else {
                writer.write_line("i32.store");
            }
        }
        
        // 3. Leave the pointer on the stack
        writer.write_line("local.get $scratch_ptr");
        Ok(())
    }

    /// Builds a member access
    pub fn build_member_access(&mut self, obj: &ExpressionNode<'a>, member: &SyntaxToken, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let obj_type_str = self.infer_expression_type(obj, function)?;
        let base_obj_type_str = if obj_type_str.ends_with("?") {
            obj_type_str[..obj_type_str.len() - 1].to_string()
        } else {
            obj_type_str.clone()
        };
        let struct_info = self.struct_table.get_struct(&base_obj_type_str).unwrap().clone();
        let field_info = struct_info.fields.get(&member.text).unwrap();
        let offset = field_info.offset;
        let wasm_type = WasmGenerator::get_wasm_type_from(field_info.type_.get_type())?;
        
        self.build_expression(obj, &obj_type_str, function, writer)?; // ptr
        
        if offset > 0 {
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add"); // ptr + offset
        }
        
        if field_info.type_.get_type() == "bool" {
            writer.write_line("i32.load8_u");
        } else if wasm_type == "f64" {
            writer.write_line("f64.load");
        } else if wasm_type == "f32" {
            writer.write_line("f32.load");
        } else {
            writer.write_line("i32.load");
        }
        
        Ok(())
    }

    /// Builds a literal value
    pub fn build_literal(&mut self, literal: &Type, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let type_ = match literal {
            Type::Integer(i) => format!("i32.const {}", i.text),
            Type::Float(f) => format!("f32.const {}", f.text),
            Type::Double(d) => format!("f64.const {}", d.text),
            Type::Boolean(f) => format!("i32.const {}", if f.text == "true" { 1 } else { 0 }),
            Type::String(s) => {
                let offset = self.strings.get(&s.text).unwrap();
                format!("i32.const {}", offset)
            },
            Type::Nullable(_) => "i32.const 0".to_string(),
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
        } else if symbol == "f64" {
            match opr.kind {
                TokenKind::SlashToken => writer.write_line(&format!("{}.div", symbol)),
                TokenKind::ModulusToken => return Err(Error::new(ErrorKind::Other, "modulus not supported for double")),
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

    pub fn build_method_call(&mut self, obj: &ExpressionNode<'a>, method: &SyntaxToken, generic_args: &Option<Vec<Type>>, params: &Vec<ExpressionNode<'a>>, _left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let obj_type = self.infer_expression_type(obj, function)?;
        
        let struct_name = if obj_type.ends_with("?") {
            obj_type[..obj_type.len() - 1].to_string()
        } else {
            obj_type.clone()
        };

        let mut mangled_name = format!("{}_{}", struct_name, method.text);

        // If it's a generic struct instance method, the struct name already has the generic type
        // The analyzer resolves it correctly

        let func_info = self.function_table.get_function(&mangled_name)?;
        
        // 1. Evaluate 'this' (the object)
        self.build_expression(obj, &obj_type, function, writer)?;
        
        // 2. Evaluate remaining parameters
        for (i, expr) in params.iter().enumerate() {
            let param_type = if i + 1 < func_info.parameters.len() {
                func_info.parameters[i + 1].clone() // i+1 because 'this' is at index 0
            } else {
                "int".to_string() // Fallback
            };
            self.build_expression(expr, &param_type, function, writer)?;
        }
        
        writer.write_line(&format!("call ${}", mangled_name));
        Ok(())
    }

    /// Builds an array literal
    pub fn build_array_literal(&mut self, elements: &Vec<ExpressionNode<'a>>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let len = elements.len();
        
        let inner_type_str = if left_side.ends_with("[]") {
            left_side[..left_side.len() - 2].to_string()
        } else {
            "int".to_string() // Fallback, shouldn't happen if semantic analysis is correct
        };
        let wasm_type = WasmGenerator::get_wasm_type_from(inner_type_str.clone())?;
        
        let element_size = match inner_type_str.as_str() {
            "bool" => 1,
            "double" => 8,
            _ => 4,
        };
        let total_size = 4 + (len * element_size); // 4 bytes for length + element_size per element

        // 1. Allocate memory using $malloc
        writer.write_line(&format!("i32.const {}", total_size));
        writer.write_line("call $malloc");
        
        // We need to store the allocated pointer in a local variable temporarily,
        // but we don't have a scratch local easily available.
        // Wait, we can just use the stack.
        // Stack: [ptr]
        
        // 2. Store the length at ptr + 0
        // We need the ptr again, but WASM doesn't have `dup`.
        // Let's use a hidden local for scratch space in every function, or just add one.
        // Since we refactored `build_function`, let's add `(local $scratch_ptr i32)` to every function.
        writer.write_line("local.set $scratch_ptr");
        
        writer.write_line("local.get $scratch_ptr"); // ptr for store
        writer.write_line(&format!("i32.const {}", len));
        writer.write_line("i32.store");
        
        // 3. Evaluate and store each element
        for (i, expr) in elements.iter().enumerate() {
            let offset = 4 + (i * element_size);
            writer.write_line("local.get $scratch_ptr"); // ptr
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add"); // ptr + offset
            
            self.build_expression(expr, &inner_type_str, function, writer)?;
            
            if inner_type_str == "bool" {
                writer.write_line("i32.store8");
            } else if wasm_type == "f64" {
                writer.write_line("f64.store");
            } else if wasm_type == "f32" {
                writer.write_line("f32.store");
            } else {
                writer.write_line("i32.store");
            }
        }
        
        // 4. Leave the pointer on the stack
        writer.write_line("local.get $scratch_ptr");
        Ok(())
    }

    /// Builds an array index access
    pub fn build_index_access(&mut self, array_expr: &ExpressionNode<'a>, index_expr: &ExpressionNode<'a>, left_side: &String, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Here left_side is the expected type of the expression, which is the inner type of the array
        let wasm_type = WasmGenerator::get_wasm_type_from(left_side.clone())?;
        
        let element_size = match left_side.as_str() {
            "bool" => 1,
            "double" => 8,
            _ => 4,
        };
        
        // Calculate the memory address: ptr + 4 + (index * element_size)
        // Note: We pass a dummy type "int[]" to build_expression for the array ptr because we just need an i32 back
        self.build_expression(array_expr, &"int[]".to_string(), function, writer)?; // ptr
        writer.write_line("i32.const 4");
        writer.write_line("i32.add"); // ptr + 4
        
        self.build_expression(index_expr, &"int".to_string(), function, writer)?; // index
        if element_size != 1 {
            writer.write_line(&format!("i32.const {}", element_size));
            writer.write_line("i32.mul"); // index * element_size
        }
        
        writer.write_line("i32.add"); // ptr + 4 + (index * element_size)
        
        // Load the value
        if left_side == "bool" {
            writer.write_line("i32.load8_u");
        } else if wasm_type == "f64" {
            writer.write_line("f64.load");
        } else if wasm_type == "f32" {
            writer.write_line("f32.load");
        } else {
            writer.write_line("i32.load");
        }
        
        Ok(())
    }
}
