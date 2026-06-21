use std::io::Error;
use crate::lang::code_analysis::syntax::nodes::{ProgramNode, FunctionNode, ParameterNode};
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds the entire WebAssembly module
    pub fn build(&mut self) -> Result<IndentedTextWriter, Error> {
        self.collect_strings_from_program(self.syntax_tree.get_root());
        self.register_object_runtime_strings();
        let mut indented = IndentedTextWriter::new();
        self.build_module(self.syntax_tree.get_root(), &mut indented)?;
        Ok(indented)
    }

    /// Builds the `(module ...)` block and its imports/exports
    pub fn build_module(&mut self, program: &ProgramNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("(module");
        writer.indent();
        
        // Import the host I/O functions (print_*) plus the importable stdlib functions.
        // `concat`/`strlen`/`debug_get_free_list_head` are compiled inline, not imported.
        let imports = crate::lang::stdlib::StdlibFunction::host_imports()
            .into_iter()
            .chain(crate::lang::stdlib::StdlibFunction::get_all());
        for std_func in imports {
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

        // User-declared `extern fun` declarations become WASM imports. The import module/field
        // default to `"env"`/<function name> but can be remapped with `@js("mod", "name")`.
        for func in program.functions.iter() {
            if !func.is_extern { continue; }

            let mut params_str = String::new();
            for p in &func.parameters {
                let resolved = self.resolve_type(&p.type_.get_type());
                params_str.push_str(&format!("{} ", WasmGenerator::get_wasm_type_from(resolved)?));
            }

            let result_str = match &func.return_type {
                Some(t) => {
                    let resolved = self.resolve_type(&t.get_type());
                    if resolved == "void" {
                        String::new()
                    } else {
                        format!(" (result {})", WasmGenerator::get_wasm_type_from(resolved)?)
                    }
                }
                None => String::new(),
            };

            let module = func.import_module.as_deref().unwrap_or("env");
            let field = func.import_name.as_deref().unwrap_or(&func.name.text);
            writer.write_line(&format!("(import \"{}\" \"{}\" (func ${} (param {}){}))",
                module, field, func.name.text, params_str.trim(), result_str));
        }

        // Memory management functions
        self.build_memory_management(writer)?;

        // Object protocol: boxing/unboxing, to_string/hash_code dispatchers, defaults.
        self.build_object_runtime(writer)?;

        // Function table for first-class function values / `call_indirect`. Every non-generic
        // top-level function (including externs) gets a stable index.
        let mut indexed_functions: Vec<&str> = Vec::new();
        for func in program.functions.iter() {
            if func.generic_parameters.is_some() { continue; }
            let name = func.name.text.as_str();
            if !self.function_indices.contains_key(name) {
                self.function_indices.insert(name.to_string(), indexed_functions.len());
                indexed_functions.push(name);
            }
        }
        if !indexed_functions.is_empty() {
            writer.write_line(&format!("(table $fn_table {} funcref)", indexed_functions.len()));
            let refs = indexed_functions.iter().map(|n| format!("${}", n)).collect::<Vec<_>>().join(" ");
            writer.write_line(&format!("(elem (i32.const 0) {})", refs));
        }

        writer.write_line("(memory 10)");
        for (s, offset) in &self.strings {
            let unquoted = if s.starts_with('"') && s.ends_with('"') {
                &s[1..s.len()-1]
            } else {
                s.as_str()
            };
            self.write_string_data(*offset, unquoted, writer);
        }
        for (content, offset) in &self.runtime_strings {
            self.write_string_data(*offset, content, writer);
        }
        
        for i in program.functions.iter() {
            if i.generic_parameters.is_some() {
                continue;
            }
            // Extern functions are imports, not definitions.
            if i.is_extern {
                continue;
            }
            self.build_function(i, writer)?;
        }
        for (mangled_name, (bindings, template)) in self.instantiated_generics {
            self.current_generic_bindings = bindings.iter().cloned().collect();
            self.current_mangled_name = Some(mangled_name.clone());
            self.build_function(template, writer)?;
            self.current_generic_bindings.clear();
            self.current_mangled_name = None;
        }
        for (method, bindings) in self.struct_methods {
            self.current_generic_bindings = bindings.iter().cloned().collect();
            self.current_mangled_name = Some(method.name.text.clone());
            self.build_function(method, writer)?;
            self.current_mangled_name = None;
            self.current_generic_bindings.clear();
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
        writer.write(" (local $scratch_len i32)");
        writer.write(" (local $scratch_arr i32)");
        writer.write(" (local $scratch_switch i32)");
        writer.write(" (local $scratch_coalesce i32)");
        writer.write_line("");
        writer.indent();

        // Take ownership of reference-typed parameters; released at every exit point below.
        self.emit_retain_params(function, writer);

        self.build_body(function.body, function, writer)?;

        // Release all local reference variables in case the function falls through without a return.
        let func_name = self.current_mangled_name.clone().unwrap_or_else(|| function.name.text.clone());
        self.emit_release_locals(&func_name, writer);

        writer.unindent();

        writer.write_line(")");
        Ok(())
    }

    /// Builds the export declarations for the module
    pub fn build_export(&self, program: &ProgramNode, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_line("(export \"memory\" (memory 0))");
        // Export the allocator so the JS interop runtime can build heap values (e.g. strings)
        // to pass back into MinLang from extern function implementations.
        writer.write_line("(export \"malloc\" (func $malloc))");
        writer.write_line("(export \"free\" (func $free))");
        for i in program.functions.iter() {
            if i.is_extern {
                continue;
            }
            if i.is_exported || i.name.text == "main" {
                writer.write_line(&format!("(export \"{}\" (func ${}))", i.name.text, i.name.text));
            }
        }
        Ok(())
    }

    /// Emits the data segments for one heap-resident string: the 12-byte block header
    /// (`size = 0`, `tag = string`, `ref_count = 1`) followed by the null-terminated bytes.
    fn write_string_data(&self, offset: usize, content: &str, writer: &mut IndentedTextWriter) {
        let header_offset = offset - super::HEAP_HEADER_SIZE;
        // size=0, tag=5 (string), ref_count=1, all little-endian i32.
        writer.write_line(&format!(
            "(data (i32.const {}) \"\\00\\00\\00\\00\\05\\00\\00\\00\\01\\00\\00\\00\")",
            header_offset
        ));
        writer.write_line(&format!("(data (i32.const {}) \"{}\\00\")", offset, content));
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
