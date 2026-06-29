//! Low-level codegen helpers shared across the WAT backend: WebAssembly type mapping, load/store
//! and refcount emission, and per-function local-variable handling. The larger concerns that used
//! to live alongside these in one "junk drawer" module now have focused submodules:
//!
//! - [`resolve`]: call/method/static-call name resolution and overload selection.
//! - [`ownership`]: reference-ownership classification and call argument/result refcount handling.
//! - [`infer`]: codegen-side best-effort expression type inference.

mod infer;
mod ownership;
mod resolve;

use super::WasmGenerator;
use crate::semantics::symbol_table::SymbolTable;
use crate::syntax::nodes::types::{release_func_suffix, strip_nullable, value_size_align};
use crate::syntax::nodes::{FunctionNode, Type};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Error;
use std::rc::Rc;

impl<'a> WasmGenerator<'a> {
    /// The byte size of a single element of the given (non-pointer) type.
    /// Pointers (arrays, structs, strings) and `int`/`float` are 4 bytes.
    pub fn element_size_of(type_name: &str) -> usize {
        value_size_align(type_name).0
    }

    /// Emits a store instruction appropriate for a value of `type_name` already on the stack
    /// (address and value must already be pushed).
    pub fn emit_store(type_name: &str, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let instruction = match WasmGenerator::get_wasm_type_from(type_name.to_string())?.as_str() {
            _ if type_name == "bool" || type_name == "char" => "i32.store8",
            "f64" => "f64.store",
            "f32" => "f32.store",
            _ => "i32.store",
        };
        writer.write_line(instruction);
        Ok(())
    }

    /// Emits a load instruction appropriate for a value of `type_name`
    /// (the address must already be on the stack).
    pub fn emit_load(type_name: &str, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let instruction = match WasmGenerator::get_wasm_type_from(type_name.to_string())?.as_str() {
            _ if type_name == "bool" || type_name == "char" => "i32.load8_u",
            "f64" => "f64.load",
            "f32" => "f32.load",
            _ => "i32.load",
        };
        writer.write_line(instruction);
        Ok(())
    }

    /// Emits a `$release_*` call for the given (possibly nullable/array) reference type.
    pub fn emit_release(&self, type_name: &str, writer: &mut IndentedTextWriter) {
        writer.write_line(&format!(
            "call $release_{}",
            release_func_suffix(strip_nullable(type_name))
        ));
    }

    /// Retains every reference-typed parameter on function entry so the matching releases at
    /// every exit point keep reference counts balanced (the callee owns its parameter bindings).
    pub fn emit_retain_params(&self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) {
        for param in &function.parameters {
            let base = strip_nullable(&self.resolve_type(&param.type_.get_type())).to_string();
            if self.is_reference_type(&base) {
                writer.write_line(&format!("local.get ${}", param.name.text));
                writer.write_line("call $retain");
            }
        }
    }

    /// Releases every reference-typed local (and parameter) recorded for `func_name`.
    /// Used both on fall-through exit and before an explicit `return`.
    pub fn emit_release_locals(&self, func_name: &str, writer: &mut IndentedTextWriter) {
        let locals = self
            .ctx
            .combined_symbol_lookup
            .get(func_name)
            .unwrap()
            .clone();
        for (name, type_) in locals.iter() {
            let base = strip_nullable(&type_.get_type()).to_string();
            if self.is_reference_type(&base) {
                writer.write_line(&format!("local.get ${}", name));
                self.emit_release(&base, writer);
            }
        }
    }
    /// Gets the WebAssembly type string from a Dream type name
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
            "double" => "f64".to_string(),
            "bool" => "i32".to_string(),
            "char" => "i32".to_string(),
            "string" => "i32".to_string(),
            "void" => "".to_string(),
            _ => {
                // If it's not a primitive, it's a struct, which is also a pointer (i32)
                "i32".to_string()
            }
        };
        Ok(r)
    }

    /// Resolves a possibly-generic type name to its concrete form during code generation,
    /// using the active monomorphization bindings. Handles `T`, `T[]`, and `T?` by stripping
    /// and re-applying the suffix around the bound base type.
    pub fn resolve_type(&self, type_str: &str) -> String {
        let (base, suffix) = if let Some(base) = type_str.strip_suffix("[]") {
            (base, "[]")
        } else if let Some(base) = type_str.strip_suffix('?') {
            (base, "?")
        } else {
            (type_str, "")
        };
        match self.ctx.current_generic_bindings.get(base) {
            Some(concrete) => format!("{}{}", concrete, suffix),
            None => type_str.to_string(),
        }
    }

    /// Reads the type of a variable from the symbol table
    pub fn table_read_type(&self, var_name: &String, function: &FunctionNode<'a>) -> String {
        let func_name = self
            .ctx
            .current_mangled_name
            .as_ref()
            .unwrap_or(&function.name.text);
        let func_lookup = self.ctx.combined_symbol_lookup.get(func_name).unwrap();
        let t = func_lookup.get(var_name).unwrap().clone().get_type();
        self.resolve_type(&t)
    }

    /// Builds local variable declarations for a function
    pub fn build_local_variable(
        &mut self,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let func_name = self
            .ctx
            .current_mangled_name
            .as_ref()
            .unwrap_or(&function.name.text)
            .clone();
        let res = self.get_local_variables(self.symbol_map.get(&func_name).unwrap())?;

        let mut param_names = std::collections::HashSet::new();
        for param in &function.parameters {
            param_names.insert(param.name.text.clone());
        }

        for (name, _type) in res.iter() {
            // Do not emit local variable declarations for function parameters
            if param_names.contains(name) {
                continue;
            }
            let resolved_type = self.resolve_type(&_type.get_type());
            writer.write(" (local ");
            writer.write(&format!(
                "${} {}",
                name,
                WasmGenerator::get_wasm_type_from(resolved_type)?
            ));
            writer.write(") ");
        }
        self.ctx.combined_symbol_lookup.insert(func_name, res);
        Ok(())
    }

    /// Gets all local variables from a symbol table and its children
    pub fn get_local_variables(
        &self,
        symbol: &Rc<RefCell<SymbolTable>>,
    ) -> Result<HashMap<String, Type>, Error> {
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
