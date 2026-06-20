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
        
        writer.write_line("(import \"env\" \"concat_strings\" (func $concat_strings (param i32 i32) (result i32)))");
        
        // Import stdlib functions
        for std_func in crate::lang::stdlib::StdlibFunction::get_all() {
            if std_func.name == "concat" { continue; } // handled by concat_strings
            
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

        // Global heap pointer for bump allocator
        writer.write_line("(global $heap_ptr (mut i32) (i32.const 1024))"); // Start heap at 1024 to leave room for static strings


        writer.write_line("(memory 1)");
        for (s, offset) in &self.strings {
            let unquoted = if s.starts_with('"') && s.ends_with('"') {
                &s[1..s.len()-1]
            } else {
                s.as_str()
            };
            writer.write_line(&format!("(data (i32.const {}) \"{}\\00\")", offset, unquoted));
        }
        
        for i in program.functions.iter() {
            self.build_function(i, writer)?;
        }
        self.build_export(program, writer)?;
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds a single WebAssembly function
    pub fn build_function(&mut self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write("(func $");
        writer.write(&function.name.text);
        for i in function.parameters.iter() {
            self.build_parameter(i, writer)?;
        }
        self.build_return_type(function, writer)?;
        self.build_local_variable(function, writer)?;
        
        // Add hidden local variable to save the heap pointer for scope-based bump allocation
        writer.write(" (local $saved_heap_ptr i32)");
        writer.write_line("");

        writer.indent();
        
        // Save the current heap pointer at the start of the function
        writer.write_line("global.get $heap_ptr");
        writer.write_line("local.set $saved_heap_ptr");
        
        self.build_body(function.body, function, writer)?;
        
        // Restore the heap pointer at the end of the function (if it doesn't return early)
        writer.write_line("local.get $saved_heap_ptr");
        writer.write_line("global.set $heap_ptr");
        
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
        writer.write(&format!("param ${} {}", parameter.name.text, WasmGenerator::get_wasm_type_from(parameter.type_.text.clone())?));
        writer.write(") ");
        Ok(())
    }

    /// Builds the return type of a function
    pub fn build_return_type(&self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        if let Some(return_type) = &function.return_type {
            let return_type_name = WasmGenerator::get_wasm_type_from(return_type.get_type())?;
            writer.write(" (result ");
            writer.write(&return_type_name);
            writer.write(")");
        }
        Ok(())
    }
}
