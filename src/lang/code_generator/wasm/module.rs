use std::io::Error;
use crate::lang::code_analysis::syntax::nodes::{ProgramNode, FunctionNode, ParameterNode};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds the entire WebAssembly module
    pub fn build(&mut self) -> Result<IndentedTextWriter, Error> {
        self.collect_strings_from_program(self.syntax_tree.get_root());
        let mut indented = IndentedTextWriter::new();
        self.build_module(self.syntax_tree.get_root(), &mut indented)?;
        Ok(indented)
    }

    /// Builds the `(module ...)` block and its imports/exports
    pub fn build_module(&mut self, program: &ProgramNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("(module");
        writer.indent();
        
        // Import stdlib functions
        for std_func in crate::lang::stdlib::StdlibFunction::get_all() {
            if std_func.name == "concat" || std_func.name == "strlen" || std_func.name == "debug_get_free_list_head" { continue; } // handled internally
            
            let mut params_str = String::new();
            for p in &std_func.parameters {
                params_str.push_str(&format!("{} ", WasmGenerator::get_wasm_type_from(p.clone())?));
            }
            
            let result_str = match &std_func.return_type {
                Some(t) => format!(" (result {})", WasmGenerator::get_wasm_type_from(t.get_type())?),
                None => "".to_string()
            };
            
            writer.write_line(&format!("(import \"env\" \"{}\" (func ${} (param {}){}))", 
                std_func.name, std_func.name, params_str.trim(), result_str));
        }

        // Memory management functions
        self.build_memory_management(writer)?;

        writer.write_line("(memory 10)");
        for (s, offset) in &self.strings {
            let unquoted = if s.starts_with('"') && s.ends_with('"') {
                &s[1..s.len()-1]
            } else {
                s.as_str()
            };
            // Write block header: size = 0, ref_count = 1
            // size is at offset - 8, ref_count is at offset - 4
            writer.write_line(&format!("(data (i32.const {}) \"\\00\\00\\00\\00\\01\\00\\00\\00\")", offset - 8));
            writer.write_line(&format!("(data (i32.const {}) \"{}\\00\")", offset, unquoted));
        }
        
        for i in program.functions.iter() {
            if i.generic_parameters.is_some() {
                continue;
            }
            self.build_function(i, writer)?;
        }
        for (mangled_name, (concrete_type, template)) in self.instantiated_generics {
            self.current_generic_type = Some(concrete_type.clone());
            self.current_mangled_name = Some(mangled_name.clone());
            self.build_function(template, writer)?;
            self.current_generic_type = None;
            self.current_mangled_name = None;
        }

        self.build_export(program, writer)?;
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds a single WebAssembly function
    pub fn build_function(&mut self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let func_name = self.current_mangled_name.as_ref().unwrap_or(&function.name.text);
        writer.write("(func $");
        writer.write(func_name);
        for i in function.parameters.iter() {
            self.build_parameter(i, writer)?;
        }
        self.build_return_type(function, writer)?;
        self.build_local_variable(function, writer)?;
        
        writer.write(" (local $scratch_ptr i32)");
        writer.write(" (local $scratch_addr i32)");
        writer.write(" (local $scratch_double f64)");
        writer.write_line("");
        writer.indent();
        
        self.build_body(function.body, function, writer)?;
        
        // Release all local reference variables in case the function falls through without a return
        let func_name = self.current_mangled_name.as_ref().unwrap_or(&function.name.text);
        let locals = self.combined_symbol_lookup.get(func_name).unwrap().clone();
        for (name, type_) in locals.iter() {
            let type_str = type_.get_type();
            let base_type_str = if type_str.ends_with("?") {
                type_str[..type_str.len() - 1].to_string()
            } else {
                type_str.clone()
            };
            
            if self.is_reference_type(&base_type_str) {
                writer.write_line(&format!("local.get ${}", name));
                writer.write_line(&format!("call $release_{}", base_type_str.replace("[]", "_array")));
            }
        }
        
        writer.unindent();

        writer.write_line(")");
        Ok(())
    }

    /// Builds the export declarations for the module
    pub fn build_export(&self, program: &ProgramNode, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("(export \"memory\" (memory 0))");
        for i in program.functions.iter() {
            if i.is_exported || i.name.text == "main" {
                writer.write_line(&format!("(export \"{}\" (func ${}))", i.name.text, i.name.text));
            }
        }
        Ok(())
    }

    /// Builds a single function parameter
    pub fn build_parameter(&self, parameter: &ParameterNode, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write("( ");
        let resolved_type = self.resolve_type(&parameter.type_.get_type());
        writer.write(&format!("param ${} {}", parameter.name.text, WasmGenerator::get_wasm_type_from(resolved_type)?));
        writer.write(") ");
        Ok(())
    }

    /// Builds the return type of a function
    pub fn build_return_type(&self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(return_type) = &function.return_type {
            let resolved_type = self.resolve_type(&return_type.get_type());
            if resolved_type != "void" {
                let return_type_name = WasmGenerator::get_wasm_type_from(resolved_type)?;
                writer.write(" (result ");
                writer.write(&return_type_name);
                writer.write(")");
            }
        }
        Ok(())
    }
}
