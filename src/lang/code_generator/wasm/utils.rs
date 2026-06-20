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
            _ => return Err(Error::new(ErrorKind::Other, format!("unsupported type {}", typename)))
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
}
