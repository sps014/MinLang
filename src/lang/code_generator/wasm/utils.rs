use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use std::cell::RefCell;
use crate::lang::code_analysis::syntax::nodes::{FunctionNode, Type};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Gets the WebAssembly type string from a MinLang type name
    pub fn get_wasm_type_from(typename: String) -> Result<String, Error> {
        let base_type = if typename.ends_with("[]") {
            // Arrays are represented as pointers (i32)
            return Ok("i32".to_string());
        } else {
            typename.as_str()
        };

        let r = match base_type {
            "int" => "i32".to_string(),
            "float" => "f32".to_string(),
            "bool" => "i32".to_string(),
            "string" => "i32".to_string(),
            "void" => "".to_string(),
            _ => {
                // If it's not a primitive, it's a struct, which is also a pointer (i32)
                "i32".to_string()
            }
        };
        Ok(r)
    }

    /// Reads the type of a variable from the symbol table
    pub fn table_read_type(&self, var_name: &String, function: &FunctionNode<'a>) -> String {
        let func_lookup = self.combined_symbol_lookup.get(&function.name.text).unwrap();
        func_lookup.get(var_name).unwrap().clone().get_type()
    }

    /// Builds local variable declarations for a function
    pub fn build_local_variable(&mut self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let res = self.get_local_variables(self.symbol_map.get(&function.name.text.clone()).unwrap())?;

        let mut param_names = std::collections::HashSet::new();
        for param in &function.parameters {
            param_names.insert(param.name.text.clone());
        }

        for (name, _type) in res.iter() {
            // Do not emit local variable declarations for function parameters
            if param_names.contains(name) {
                continue;
            }
            writer.write(" (local ");
            writer.write(&format!("${} {}", name, WasmGenerator::get_wasm_type_from(_type.get_type())?));
            writer.write(") ");
        }
        self.combined_symbol_lookup.insert(function.name.text.clone(), res);
        Ok(())
    }

    /// Gets all local variables from a symbol table and its children
    pub fn get_local_variables(&self, symbol: &Rc<RefCell<SymbolTable>>) -> Result<HashMap<String, Type>, Error> {
        let mut res = HashMap::new();
        let current_scope = (*symbol).as_ref().borrow();
        let mut local_variables = current_scope.get_all();

        for children in current_scope.children.iter() {
            let child_local_variables = self.get_local_variables(children)?;
            local_variables.extend(child_local_variables);
        }
        
        for (name, type_) in local_variables.iter() {
            if !res.contains_key(name) {
                res.insert(name.clone(), type_.clone());
            }
        }

        Ok(res)
    }

    /// Infers the type of an expression (simplified version of semantic analyzer)
    pub fn infer_expression_type(&self, expression: &crate::lang::code_analysis::syntax::nodes::ExpressionNode<'a>, function: &FunctionNode<'a>) -> Result<String, Error> {
        use crate::lang::code_analysis::syntax::nodes::ExpressionNode;
        match expression {
            ExpressionNode::Literal(t) => Ok(t.get_type()),
            ExpressionNode::Identifier(id) => Ok(self.table_read_type(&id.text, function)),
            ExpressionNode::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    Ok("void[]".to_string())
                } else {
                    let inner = self.infer_expression_type(&elements[0], function)?;
                    Ok(format!("{}[]", inner))
                }
            },
            ExpressionNode::IndexAccess(arr, _) => {
                let arr_type = self.infer_expression_type(arr, function)?;
                if arr_type.ends_with("[]") {
                    Ok(arr_type[0..arr_type.len()-2].to_string())
                } else {
                    Ok("void".to_string())
                }
            },
            ExpressionNode::FunctionCall(name, _) => {
                if let Ok(func) = self.function_table.get_function(&name.text) {
                    if let Some(ret_type) = &func.return_type {
                        Ok(ret_type.get_type())
                    } else {
                        Ok("void".to_string())
                    }
                } else {
                    // Check stdlib
                    for std_func in crate::lang::stdlib::StdlibFunction::get_all() {
                        if std_func.name == name.text {
                            if let Some(ret_type) = &std_func.return_type {
                                return Ok(ret_type.get_type());
                            } else {
                                return Ok("void".to_string());
                            }
                        }
                    }
                    Ok("void".to_string())
                }
            },
            ExpressionNode::Unary(_, right) => self.infer_expression_type(right, function),
            ExpressionNode::Binary(left, opr, _) => {
                use crate::lang::code_analysis::token::token_kind::TokenKind;
                match opr.kind {
                    TokenKind::EqualEqualToken | TokenKind::NotEqualToken |
                    TokenKind::GreaterThanToken | TokenKind::SmallerThanToken |
                    TokenKind::GreaterThanEqualToken | TokenKind::SmallerThanEqualToken |
                    TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken => Ok("bool".to_string()),
                    _ => self.infer_expression_type(left, function)
                }
            },
            ExpressionNode::Parenthesized(expr) => self.infer_expression_type(expr, function),
            ExpressionNode::Cast(target_type, _) => Ok(target_type.get_type()),
            ExpressionNode::StructInstantiation(name, _) => Ok(name.text.clone()),
            ExpressionNode::MemberAccess(obj, member) => {
                let obj_type = self.infer_expression_type(obj, function)?;
                if let Some(struct_info) = self.struct_table.get_struct(&obj_type) {
                    if let Some(field_info) = struct_info.fields.get(&member.text) {
                        return Ok(field_info.type_.get_type());
                    }
                }
                Ok("void".to_string())
            },
        }
    }
}
