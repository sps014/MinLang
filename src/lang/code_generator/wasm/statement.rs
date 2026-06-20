use std::io::Error;
use crate::lang::code_analysis::syntax::nodes::{StatementNode, FunctionNode, ExpressionNode};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds the body of a function
    pub fn build_body(&mut self, statements: &[StatementNode<'a>], function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        for i in statements.iter() {
            self.build_statement(i, function, writer)?;
        }
        Ok(())
    }

    /// Builds a single statement
    pub fn build_statement(&mut self, statement: &StatementNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        match statement {
            StatementNode::Declaration(left, _, expression) => self.build_declaration(left, function, expression, writer)?,
            StatementNode::Assignment(left, expression) => self.build_assignment(left, expression, function, writer)?,
            StatementNode::IndexAssignment(left, index, expression) => self.build_index_assignment(left, index, expression, function, writer)?,
            StatementNode::MemberAssignment(obj, member, expression) => self.build_member_assignment(obj, member, expression, function, writer)?,
            StatementNode::Return(r) => self.build_return(r, function, writer)?,
            StatementNode::While(c, b) => self.build_while(c, b, function, writer)?,
            StatementNode::For(init, cond, inc, body) => self.build_for(init, cond, inc, body, function, writer)?,
            StatementNode::Break => self.build_break(writer)?,
            StatementNode::Continue => self.build_continue(writer)?,
            StatementNode::IfElse(c, b, else_if, else_b) => self.build_if_else(c, b, else_if, else_b, function, writer)?,
            StatementNode::FunctionInvocation(n, generic_args, p) => {
                let mut function_name = n.text.clone();
                // If it's a generic call, mangle the name
                if let Some(generics) = generic_args {
                    if !generics.is_empty() {
                        let type_str = generics[0].get_type();
                        function_name = format!("{}_{}", function_name, type_str);
                    }
                } else if self.function_table.get_function(&function_name).is_err() {
                    // Try to infer generic type from first argument if not explicit
                    if !p.is_empty() {
                        if let Ok(inferred_type) = self.infer_expression_type(&p[0], function) {
                            let mangled = format!("{}_{}", function_name, inferred_type);
                            if self.function_table.get_function(&mangled).is_ok() {
                                function_name = mangled;
                            }
                        }
                    }
                }
                self.build_function_invocation(&function_name, p, function, writer)?
            },
            StatementNode::MethodInvocation(obj, method, generic_args, params) => {
                // Since it's an invocation as a statement, we don't care about the return value.
                // We just call the method using build_method_call.
                self.build_method_call(obj, method, generic_args, params, &"void".to_string(), function, writer)?;
                // If the method returns a value, we should theoretically drop it, 
                // but WASM requires `drop` if the function returns something.
                // MinLang function invocation currently doesn't drop values.
                // We will leave it as is, or we could look up the return type.
                let obj_type = self.infer_expression_type(obj, function)?;
                let struct_name = if obj_type.ends_with("?") {
                    obj_type[..obj_type.len() - 1].to_string()
                } else {
                    obj_type.clone()
                };
                let mangled_name = format!("{}_{}", struct_name, method.text);
                if let Ok(func_info) = self.function_table.get_function(&mangled_name) {
                    if func_info.return_type.is_some() && func_info.return_type.as_ref().unwrap().get_type() != "void" {
                        writer.write_line("drop");
                    }
                }
            },
        }
        Ok(())
    }

    /// Builds a variable declaration
    pub fn build_declaration(&mut self, left: &SyntaxToken, function: &FunctionNode<'a>, expression: &ExpressionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let type_str = self.table_read_type(&left.text, function);
        self.build_expression(expression, &type_str, function, writer)?;
        
        if self.is_reference_type(&type_str) {
            writer.write_line("local.set $scratch_ptr");
            writer.write_line("local.get $scratch_ptr");
            writer.write_line("call $retain");
            
            // Release old value (in case this declaration is inside a loop and the local is reused)
            writer.write_line(&format!("local.get ${}", left.text));
            writer.write_line(&format!("call $release_{}", type_str.replace("[]", "_array").replace("?", "")));
            
            writer.write_line("local.get $scratch_ptr");
        }
        
        writer.write_line(&format!("local.set ${}", left.text));
        Ok(())
    }

    /// Builds a variable assignment
    pub fn build_assignment(&mut self, left: &SyntaxToken, expression: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let type_str = self.table_read_type(&left.text, function);
        
        // Evaluate new value
        self.build_expression(expression, &type_str, function, writer)?;
        
        if self.is_reference_type(&type_str) {
            writer.write_line("local.set $scratch_ptr");
            
            // Retain new value
            writer.write_line("local.get $scratch_ptr");
            writer.write_line("call $retain");
            
            // Release old value
            writer.write_line(&format!("local.get ${}", left.text));
            writer.write_line(&format!("call $release_{}", type_str.replace("[]", "_array").replace("?", "")));
            
            // Store new value
            writer.write_line("local.get $scratch_ptr");
        }
        
        writer.write_line(&format!("local.set ${}", left.text));
        Ok(())
    }

    /// Builds an array index assignment
    pub fn build_index_assignment(&mut self, arr: &ExpressionNode<'a>, index: &ExpressionNode<'a>, expression: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let array_type_str = self.infer_expression_type(arr, function)?;
        let inner_type_str = array_type_str[..array_type_str.len() - 2].to_string();
        let wasm_type = WasmGenerator::get_wasm_type_from(inner_type_str.clone())?;
        
        let element_size = match inner_type_str.as_str() {
            "bool" => 1,
            "double" => 8,
            _ => 4,
        };
        
        // Calculate the memory address: ptr + 4 + (index * element_size)
        self.build_expression(arr, &array_type_str, function, writer)?;
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        self.build_expression(index, &"int".to_string(), function, writer)?;
        if element_size != 1 {
            writer.write_line(&format!("i32.const {}", element_size));
            writer.write_line("i32.mul");
        }
        writer.write_line("i32.add");
        writer.write_line("local.set $scratch_addr");
        
        if self.is_reference_type(&inner_type_str) {
            self.build_expression(expression, &inner_type_str, function, writer)?;
            writer.write_line("local.set $scratch_ptr");
            
            writer.write_line("local.get $scratch_ptr");
            writer.write_line("call $retain");
            
            writer.write_line("local.get $scratch_addr");
            writer.write_line("i32.load");
            writer.write_line(&format!("call $release_{}", inner_type_str.replace("[]", "_array").replace("?", "")));
            
            writer.write_line("local.get $scratch_addr");
            writer.write_line("local.get $scratch_ptr");
        } else {
            writer.write_line("local.get $scratch_addr");
            self.build_expression(expression, &inner_type_str, function, writer)?;
        }
        
        if inner_type_str == "bool" {
            writer.write_line("i32.store8");
        } else if wasm_type == "f64" {
            writer.write_line("f64.store");
        } else if wasm_type == "f32" {
            writer.write_line("f32.store");
        } else {
            writer.write_line("i32.store");
        }
        
        Ok(())
    }

    /// Builds a member assignment
    pub fn build_member_assignment(&mut self, obj: &ExpressionNode<'a>, member: &SyntaxToken, expression: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let obj_type_str = self.infer_expression_type(obj, function)?;
        let base_obj_type_str = if obj_type_str.ends_with("?") {
            obj_type_str[..obj_type_str.len() - 1].to_string()
        } else {
            obj_type_str.clone()
        };
        let struct_info = self.struct_table.get_struct(&base_obj_type_str).unwrap().clone();
        let field_info = struct_info.fields.get(&member.text).unwrap();
        let offset = field_info.offset;
        let field_type_str = field_info.type_.get_type();
        let wasm_type = WasmGenerator::get_wasm_type_from(field_type_str.clone())?;
        
        // Address
        self.build_expression(obj, &obj_type_str, function, writer)?;
        if offset > 0 {
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add");
        }
        writer.write_line("local.set $scratch_addr");
        
        if self.is_reference_type(&field_type_str) {
            self.build_expression(expression, &field_type_str, function, writer)?;
            writer.write_line("local.set $scratch_ptr");
            
            writer.write_line("local.get $scratch_ptr");
            writer.write_line("call $retain");
            
            writer.write_line("local.get $scratch_addr");
            writer.write_line("i32.load");
            writer.write_line(&format!("call $release_{}", field_type_str.replace("[]", "_array").replace("?", "")));
            
            writer.write_line("local.get $scratch_addr");
            writer.write_line("local.get $scratch_ptr");
        } else {
            writer.write_line("local.get $scratch_addr");
            self.build_expression(expression, &field_type_str, function, writer)?;
        }
        
        if field_type_str == "bool" {
            writer.write_line("i32.store8");
        } else if wasm_type == "f64" {
            writer.write_line("f64.store");
        } else if wasm_type == "f32" {
            writer.write_line("f32.store");
        } else {
            writer.write_line("i32.store");
        }
        
        Ok(())
    }

    /// Builds a return statement
    pub fn build_return(&mut self, expression: &Option<ExpressionNode<'a>>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(expr) = expression {
            let return_type = function.return_type.as_ref().unwrap();
            self.build_expression(expr, &return_type.get_type(), function, writer)?;
            
            // If returning a reference type, we need to retain it so it survives the scope exit
            let ret_type_str = return_type.get_type();
            let base_ret_type_str = if ret_type_str.ends_with("?") {
                ret_type_str[..ret_type_str.len() - 1].to_string()
            } else {
                ret_type_str.clone()
            };
            
            if self.is_reference_type(&base_ret_type_str) {
                // Store in scratch_ptr, retain it, then we will release locals, then put it back on stack
                writer.write_line("local.set $scratch_ptr");
                writer.write_line("local.get $scratch_ptr");
                writer.write_line("call $retain");
            } else if return_type.get_type() == "double" {
                writer.write_line("local.set $scratch_double");
            } else {
                writer.write_line("local.set $scratch_ptr"); // using scratch_ptr for i32/f32 return values temporarily
            }
        }
        
        // Release all local reference variables in the current function scope
        let locals = self.combined_symbol_lookup.get(&function.name.text).unwrap().clone();
        for (name, type_) in locals.iter() {
            let type_str = type_.get_type();
            let base_type_str = if type_str.ends_with("?") {
                type_str[..type_str.len() - 1].to_string()
            } else {
                type_str.clone()
            };
            
            if self.is_reference_type(&base_type_str) {
                writer.write_line(&format!("local.get ${}", name));
                writer.write_line(&format!("call $release_{}", base_type_str.replace("[]", "_array").replace("?", "")));
            }
        }

        if let Some(_) = expression {
            let return_type = function.return_type.as_ref().unwrap();
            if return_type.get_type() == "double" {
                writer.write_line("local.get $scratch_double");
            } else {
                writer.write_line("local.get $scratch_ptr");
            }
        }
        
        writer.write_line("return");
        Ok(())
    }

    /// Builds a while loop
    pub fn build_while(&mut self, condition: &ExpressionNode<'a>, body: &[StatementNode<'a>], function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let loop_id = self.loop_counter;
        self.loop_counter += 1;
        self.loop_stack.push(loop_id);
        
        writer.write_line(&format!("(block $loop_end_{}", loop_id));
        writer.indent();
        writer.write_line(&format!("(loop $loop_start_{}", loop_id));
        writer.indent();
        self.build_expression(condition, &"int".to_string(), function, writer)?;
        writer.write_line("i32.const 0");
        writer.write_line("i32.eq");
        writer.write_line(&format!("br_if $loop_end_{}", loop_id));
        
        writer.write_line(&format!("(block $continue_target_{}", loop_id));
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");
        
        writer.write_line(&format!("br $loop_start_{}", loop_id));
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        
        self.loop_stack.pop();
        Ok(())
    }

    /// Builds a for loop
    pub fn build_for(&mut self, init: &Option<&'a StatementNode<'a>>, condition: &Option<ExpressionNode<'a>>, increment: &Option<&'a StatementNode<'a>>, body: &[StatementNode<'a>], function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(init_stmt) = init {
            self.build_statement(init_stmt, function, writer)?;
        }
        
        let loop_id = self.loop_counter;
        self.loop_counter += 1;
        self.loop_stack.push(loop_id);
        
        writer.write_line(&format!("(block $loop_end_{}", loop_id));
        writer.indent();
        writer.write_line(&format!("(loop $loop_start_{}", loop_id));
        writer.indent();
        
        if let Some(cond_expr) = condition {
            self.build_expression(cond_expr, &"int".to_string(), function, writer)?;
            writer.write_line("i32.const 0");
            writer.write_line("i32.eq");
            writer.write_line(&format!("br_if $loop_end_{}", loop_id));
        }
        
        writer.write_line(&format!("(block $continue_target_{}", loop_id));
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");
        
        if let Some(inc_stmt) = increment {
            self.build_statement(inc_stmt, function, writer)?;
        }
        
        writer.write_line(&format!("br $loop_start_{}", loop_id));
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        
        self.loop_stack.pop();
        Ok(())
    }

    /// Builds a break statement
    pub fn build_break(&mut self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(loop_id) = self.loop_stack.last() {
            writer.write_line(&format!("br $loop_end_{}", loop_id));
        } else {
            writer.write_line("br 1"); // Fallback, though semantic analyzer should catch this
        }
        Ok(())
    }

    /// Builds a continue statement
    pub fn build_continue(&mut self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(loop_id) = self.loop_stack.last() {
            writer.write_line(&format!("br $continue_target_{}", loop_id));
        } else {
            writer.write_line("br 0"); // Fallback
        }
        Ok(())
    }

    /// Builds an if-else statement
    pub fn build_if_else(&mut self, condition: &ExpressionNode<'a>, body: &'a [StatementNode<'a>], else_if: &Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>, else_body: &Option<&'a [StatementNode<'a>]>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
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
    pub fn build_if_else_parts(&mut self, parts: &Vec<(Option<&ExpressionNode<'a>>, &'a [StatementNode<'a>])>, function: &FunctionNode<'a>, index: usize, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if index == parts.len() {
            return Ok(());
        }
        let cur = &parts[index];
        if cur.0.is_none() && index == parts.len() - 1 {
            self.build_body(cur.1, function, writer)?;
        } else {
            let mut is_constant_true = false;
            let mut is_constant_false = false;
            if let Some(ExpressionNode::IsExpression(left, right_type)) = cur.0 {
                let left_type = self.infer_expression_type(left, function)?;
                if left_type == right_type.get_type() {
                    is_constant_true = true;
                } else {
                    is_constant_false = true;
                }
            }

            if is_constant_true {
                self.build_body(cur.1, function, writer)?;
                return Ok(());
            } else if is_constant_false {
                self.build_if_else_parts(parts, function, index + 1, writer)?;
                return Ok(());
            }

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
