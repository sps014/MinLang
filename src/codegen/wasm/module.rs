use super::WasmGenerator;
use crate::syntax::nodes::{FunctionNode, ParameterNode, ProgramNode};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use std::io::Error;

/// The reference count stamped into a heap block's header when it is created. Statically
/// allocated blocks (e.g. string literals) start "live" with a single owning reference.
const INITIAL_REF_COUNT: i32 = 1;

/// Formats a 32-bit value as the four little-endian bytes of a WAT data-segment string literal
/// (e.g. `5` -> `\05\00\00\00`), so numeric header fields can be written without hand-encoding.
fn le_i32_bytes(value: i32) -> String {
    value
        .to_le_bytes()
        .iter()
        .map(|b| format!("\\{:02x}", b))
        .collect()
}

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
    pub fn build_module(
        &mut self,
        program: &ProgramNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        writer.write_line("(module");
        writer.indent();

        // Import the host I/O functions (print_*) plus the importable stdlib functions. Functions
        // flagged `inline` (the string/char runtime helpers, compiled into RUNTIME_STRINGS) are not
        // imported - the `inline` field on StdlibFunction is the single source of truth for that.
        let imports = crate::stdlib::StdlibFunction::host_imports()
            .into_iter()
            .chain(crate::stdlib::StdlibFunction::get_all());
        for std_func in imports {
            if std_func.inline {
                continue;
            } // body emitted internally, not imported

            let mut params_str = String::new();
            for p in &std_func.parameters {
                params_str.push_str(&format!(
                    "{} ",
                    WasmGenerator::get_wasm_type_from(p.clone())?
                ));
            }

            let result_str = match &std_func.return_type {
                Some(t) => format!(
                    " (result {})",
                    WasmGenerator::get_wasm_type_from(t.get_type())?
                ),
                None => "".to_string(),
            };

            writer.write_line(&format!(
                "(import \"env\" \"{}\" (func ${} (param {}){}))",
                std_func.name,
                std_func.name,
                params_str.trim(),
                result_str
            ));
        }

        // User-declared `extern fun` declarations become WASM imports. The import module/field
        // default to `"env"`/<function name> but can be remapped with `@js("mod", "name")`.
        for func in program.functions.iter() {
            if !func.is_extern {
                continue;
            }

            let mut params_str = String::new();
            for p in &func.parameters {
                let resolved = self.resolve_type(&p.type_.get_type());
                params_str.push_str(&format!(
                    "{} ",
                    WasmGenerator::get_wasm_type_from(resolved)?
                ));
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

            let mut import_module = "env";
            let mut import_name = func.name.text.as_str();
            if let Some(js_attr) = func.attributes.iter().find(|a| a.name.text == "js") {
                if let Some(arg) = js_attr.args.get(0) {
                    import_module = arg.text.trim_matches('"');
                }
                if let Some(arg) = js_attr.args.get(1) {
                    import_name = arg.text.trim_matches('"');
                }
            }
            let module = import_module;
            let field = import_name;
            // Overloaded externs get distinct internal `$key` names but share the imported field,
            // so a single host function can back every signature.
            let internal = self
                .function_table
                .resolve_emitted_name(&func.name.text, &Self::func_param_types(func));
            writer.write_line(&format!(
                "(import \"{}\" \"{}\" (func ${} (param {}){}))",
                module,
                field,
                internal,
                params_str.trim(),
                result_str
            ));
        }

        // Memory management functions
        self.build_memory_management(writer)?;

        // Object protocol: boxing/unboxing, to_string/hash_code dispatchers, defaults.
        self.build_object_runtime(writer)?;

        // Per-enum `name()` lookup functions.
        self.build_enum_runtime(writer);

        // Function table for first-class function values / `call_indirect`. Every non-generic
        // top-level function (including externs) gets a stable index.
        let mut indexed_functions: Vec<&str> = Vec::new();
        for func in program.functions.iter() {
            if func.generic_parameters.is_some() {
                continue;
            }
            let name = func.name.text.as_str();
            // Overloaded names are ambiguous as first-class function values, so they get no slot.
            if self.function_table.is_overloaded(name) {
                continue;
            }
            if !self.ctx.function_indices.contains_key(name) {
                self.ctx
                    .function_indices
                    .insert(name.to_string(), indexed_functions.len());
                indexed_functions.push(name);
            }
        }
        // Async poll functions share `$fn_table`: each `async fun` gets a slot after the
        // first-class function slots so the scheduler can resume it via `call_indirect`.
        let mut poll_refs: Vec<String> = Vec::new();
        for func in program.functions.iter() {
            if !func.is_async || func.is_extern || func.generic_parameters.is_some() {
                continue;
            }
            let emitted = self
                .function_table
                .resolve_emitted_name(&func.name.text, &Self::func_param_types(func));
            let idx = indexed_functions.len() + poll_refs.len();
            self.ctx.poll_indices.insert(emitted.clone(), idx);
            poll_refs.push(format!("$poll_{}", emitted));
            self.ctx.has_async = true;
        }
        // Async class/extension methods also need a poll slot (resolved under their mangled
        // `Type_method` key, parameter list including the implicit `this`).
        for (method, _bindings) in self.struct_methods.iter() {
            if !method.is_async || method.is_extern {
                continue;
            }
            let emitted = self
                .function_table
                .resolve_emitted_name(&method.name.text, &Self::func_param_types(method));
            if self.ctx.poll_indices.contains_key(&emitted) {
                continue;
            }
            let idx = indexed_functions.len() + poll_refs.len();
            self.ctx.poll_indices.insert(emitted.clone(), idx);
            poll_refs.push(format!("$poll_{}", emitted));
            self.ctx.has_async = true;
        }
        let total_table = indexed_functions.len() + poll_refs.len();
        if total_table > 0 {
            writer.write_line(&format!("(table $fn_table {} funcref)", total_table));
            let mut all_refs: Vec<String> = indexed_functions
                .iter()
                .map(|n| format!("${}", n))
                .collect();
            all_refs.extend(poll_refs);
            writer.write_line(&format!("(elem (i32.const 0) {})", all_refs.join(" ")));
        }
        if self.ctx.has_async {
            self.build_async_runtime(writer);
        }

        writer.write_line("(memory 10)");
        for (s, offset) in &self.ctx.strings {
            let unquoted = if s.starts_with('"') && s.ends_with('"') {
                &s[1..s.len() - 1]
            } else {
                s.as_str()
            };
            self.write_string_data(*offset, unquoted, writer);
        }
        for (content, offset) in &self.ctx.runtime_strings {
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
            // Overloaded functions are emitted under their signature-mangled key.
            self.ctx.current_mangled_name = Some(
                self.function_table
                    .resolve_emitted_name(&i.name.text, &Self::func_param_types(i)),
            );
            if i.is_async {
                self.build_async_function(i, writer)?;
            } else {
                self.build_function(i, writer)?;
            }
            self.ctx.current_mangled_name = None;
        }
        for (mangled_name, (bindings, template)) in self.instantiated_generics {
            self.ctx.current_generic_bindings = bindings.iter().cloned().collect();
            self.ctx.current_mangled_name = Some(mangled_name.clone());
            self.build_function(template, writer)?;
            self.ctx.current_generic_bindings.clear();
            self.ctx.current_mangled_name = None;
        }
        for (method, bindings) in self.struct_methods {
            self.ctx.current_generic_bindings = bindings.iter().cloned().collect();
            // Overloaded methods are emitted under their signature-mangled key (the parameter
            // list includes the implicit `this`, matching how they were registered).
            self.ctx.current_mangled_name = Some(
                self.function_table
                    .resolve_emitted_name(&method.name.text, &Self::func_param_types(method)),
            );
            if method.is_async {
                self.build_async_function(method, writer)?;
            } else {
                self.build_function(method, writer)?;
            }
            self.ctx.current_mangled_name = None;
            self.ctx.current_generic_bindings.clear();
        }

        self.build_export(program, writer)?;
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds a single WebAssembly function
    pub fn build_function(
        &mut self,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let func_name = self
            .ctx
            .current_mangled_name
            .as_ref()
            .unwrap_or(&function.name.text);
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
        writer.write(" (local $scratch_float f32)");
        writer.write(" (local $scratch_len i32)");
        writer.write(" (local $scratch_arr i32)");
        writer.write(" (local $scratch_switch i32)");
        writer.write(" (local $scratch_coalesce i32)");
        // Base-pointer locals for nested heap constructors (see `ctor_base_local`).
        for i in 0..Self::CTOR_BASE_POOL {
            writer.write(&format!(" (local $ctor_base{} i32)", i));
        }
        // Temp locals holding owned-reference call arguments so they can be released after the
        // call (see `build_call_arg` / `release_call_temps`).
        for i in 0..Self::TMP_POOL {
            writer.write(&format!(" (local $tmp{} i32)", i));
        }
        writer.write_line("");
        writer.indent();

        // Take ownership of reference-typed parameters; released at every exit point below.
        self.emit_retain_params(function, writer);

        self.build_body(function.body, function, writer)?;

        // Release all local reference variables in case the function falls through without a return.
        let func_name = self
            .ctx
            .current_mangled_name
            .clone()
            .unwrap_or_else(|| function.name.text.clone());
        self.emit_release_locals(&func_name, writer);

        writer.unindent();

        writer.write_line(")");
        Ok(())
    }

    /// Builds the export declarations for the module
    pub fn build_export(
        &self,
        program: &ProgramNode,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        writer.write_line("(export \"memory\" (memory 0))");
        // Export the allocator so the JS interop runtime can build heap values (e.g. strings)
        // to pass back into Dream from extern function implementations.
        writer.write_line("(export \"malloc\" (func $malloc))");
        writer.write_line("(export \"free\" (func $free))");
        // Scheduler entry points for hosts (dream.js) to pump the loop, resolve host promises
        // created by `extern async` imports, and allocate those host futures.
        if self.ctx.has_async {
            writer.write_line("(export \"__dream_run_loop\" (func $dream_run_loop))");
            writer.write_line("(export \"__dream_resolve\" (func $dream_resolve))");
            writer.write_line("(export \"__dream_new_future\" (func $dream_new_future))");
        }
        // Export the indirect function table so the JS runtime can invoke a Dream function passed
        // to a `fun(...)`-typed extern parameter (a Dream -> JS callback), via `table.get(idx)`.
        if !self.ctx.function_indices.is_empty() || !self.ctx.poll_indices.is_empty() {
            writer.write_line("(export \"__indirect_function_table\" (table $fn_table))");
        }
        for i in program.functions.iter() {
            if i.is_extern {
                continue;
            }
            if i.is_exported || i.name.text == "main" {
                let emitted = self
                    .function_table
                    .resolve_emitted_name(&i.name.text, &Self::func_param_types(i));

                // An async `main` is invoked as `() -> ()`: spawn the top-level task (the
                // constructor eagerly enqueues it) and pump the scheduler to completion. Any
                // declared `args: string[]` is forwarded as an empty array.
                if i.name.text == "main" && i.is_async {
                    writer.write_line("(func (export \"main\")");
                    writer.indent();
                    if !i.parameters.is_empty() {
                        writer.write_line("(local $args i32)");
                        writer.write_line("i32.const 4");
                        writer.write_line(&format!("i32.const {}", super::object::TAG_ARRAY));
                        writer.write_line("call $malloc");
                        writer.write_line("local.set $args");
                        writer.write_line("local.get $args");
                        writer.write_line("i32.const 0");
                        writer.write_line("i32.store");
                        writer.write_line("local.get $args");
                    }
                    writer.write_line(&format!("call ${}", emitted));
                    writer.write_line("drop");
                    writer.write_line("call $dream_run_loop");
                    writer.unindent();
                    writer.write_line(")");
                    continue;
                }

                // `main(args: string[])`: the host runner invokes `main` as `() -> ()`, so instead
                // of exporting the user `$main` (which takes an array pointer) we export a synthetic
                // zero-arg wrapper that allocates an empty `string[]`, forwards it, drops any return,
                // and releases the array.
                if i.name.text == "main" && !i.parameters.is_empty() {
                    writer.write_line("(func (export \"main\")");
                    writer.indent();
                    writer.write_line("(local $args i32)");
                    // Allocate a zero-length array: 4-byte length word, TAG_ARRAY block.
                    writer.write_line("i32.const 4");
                    writer.write_line(&format!("i32.const {}", super::object::TAG_ARRAY));
                    writer.write_line("call $malloc");
                    writer.write_line("local.set $args");
                    writer.write_line("local.get $args");
                    writer.write_line("i32.const 0");
                    writer.write_line("i32.store");
                    // Forward to the user entry point.
                    writer.write_line("local.get $args");
                    writer.write_line(&format!("call ${}", emitted));
                    if i.return_type
                        .as_ref()
                        .map(|t| t.get_type() != "void")
                        .unwrap_or(false)
                    {
                        writer.write_line("drop");
                    }
                    writer.write_line("local.get $args");
                    writer.write_line("call $release_generic");
                    writer.unindent();
                    writer.write_line(")");
                    continue;
                }

                // Overloaded exports are surfaced under their mangled key so export names stay unique.
                let export_label = if self.function_table.is_overloaded(&i.name.text) {
                    emitted.clone()
                } else {
                    i.name.text.clone()
                };
                writer.write_line(&format!(
                    "(export \"{}\" (func ${}))",
                    export_label, emitted
                ));
            }
        }
        Ok(())
    }

    /// The declared parameter type names of `func` (no monomorphization), matching the keys used
    /// when the function/method was registered in the function table.
    fn func_param_types(func: &FunctionNode) -> Vec<String> {
        func.parameters.iter().map(|p| p.type_.get_type()).collect()
    }

    /// Emits the data segments for one heap-resident string: the 12-byte block header
    /// (`size = 0`, `tag = string`, `ref_count = 1`) followed by the null-terminated bytes.
    fn write_string_data(&self, offset: usize, content: &str, writer: &mut IndentedTextWriter) {
        let header_offset = offset - super::HEAP_HEADER_SIZE;
        // Block header: size=0, tag=string, ref_count=1, each a little-endian i32. The tag is
        // sourced from `object::TAG_STRING` so it can never drift from the runtime's view.
        let header = format!(
            "{}{}{}",
            le_i32_bytes(0),
            le_i32_bytes(super::object::TAG_STRING),
            le_i32_bytes(INITIAL_REF_COUNT),
        );
        writer.write_line(&format!(
            "(data (i32.const {}) \"{}\")",
            header_offset, header
        ));
        writer.write_line(&format!(
            "(data (i32.const {}) \"{}\\00\")",
            offset, content
        ));
    }

    /// Builds a single function parameter
    pub fn build_parameter(
        &self,
        parameter: &ParameterNode,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        writer.write("( ");
        let resolved_type = self.resolve_type(&parameter.type_.get_type());
        writer.write(&format!(
            "param ${} {}",
            parameter.name.text,
            WasmGenerator::get_wasm_type_from(resolved_type)?
        ));
        writer.write(") ");
        Ok(())
    }

    /// Builds the return type of a function
    pub fn build_return_type(
        &self,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
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
