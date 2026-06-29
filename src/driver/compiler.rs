use bumpalo::Bump;
use std::collections::HashSet;
use std::fs;
use std::io::Error;
use tracing::info;

use crate::codegen::wasm::WasmGenerator;
use crate::codegen::CodeGenerator;
use crate::driver::abi::emit_wasm_and_abi;
use crate::driver::diagnostics::{self, DiagnosticBag};
use crate::driver::source_manager::{generate_json_derives, merge_prelude, parse_file_recursive};
use crate::semantics::analyzer::Analyzer;
use crate::syntax::nodes::ProgramNode;
use crate::syntax::syntax_tree::SyntaxTree;

pub enum Target {
    Wasm,
}

/// Orchestrates the compilation pipeline: source loading (delegated to `source_manager`),
/// semantic analysis, code generation, and artifact emission (delegated to `abi`). Diagnostic
/// rendering is delegated to the `diagnostics` module.
pub struct Compiler {
    target: Target,
}

impl Compiler {
    pub fn new(target: Target) -> Self {
        Self { target }
    }

    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), Error> {
        info!("starting parsing and multi-file resolution");
        let mut visited_files = HashSet::new();
        let mut all_functions = vec![];
        let mut all_structs = vec![];
        let mut all_enums = vec![];
        let mut all_extends = vec![];
        let mut file_contents = std::collections::HashMap::new();

        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);

        parse_file_recursive(
            main_file_path,
            &mut visited_files,
            &mut all_functions,
            &mut all_structs,
            &mut all_enums,
            &mut all_extends,
            &arena,
            &mut diagnostics,
            &mut file_contents,
        )?;

        // The standard collections (List<T>, Map<K, V>) are embedded in the compiler and merged
        // into every program as a prelude. They are generic templates, so they emit no code unless
        // the program actually instantiates them.
        merge_prelude(
            &arena,
            &mut all_functions,
            &mut all_structs,
            &mut all_extends,
            &mut diagnostics,
            &mut file_contents,
        )?;

        // Auto-derive `to_json`/`from_json` converters for every `@json` class (must run after
        // all classes are collected so `@json` field cross-references resolve).
        generate_json_derives(
            &arena,
            &all_structs,
            &mut all_extends,
            &mut diagnostics,
            &mut file_contents,
        )?;

        if diagnostics.has_errors() {
            diagnostics::render(&diagnostics, &file_contents);
            return Err(Error::other(
                "Syntax errors found during parsing",
            ));
        }

        let combined_program =
            ProgramNode::new(vec![], all_structs, all_functions, all_enums, all_extends);
        let ast = SyntaxTree::new(combined_program);

        info!("finished parsing");
        info!("starting semantic analysis");

        let mut analyzer = Analyzer::new(&ast, &arena);
        let symbol_info = match analyzer.analyze(&mut diagnostics) {
            Ok(info) => info,
            Err(_) => {
                diagnostics::render(&diagnostics, &file_contents);
                return Err(Error::other("Semantic errors found"));
            }
        };

        if diagnostics.has_errors() {
            diagnostics::render(&diagnostics, &file_contents);
            return Err(Error::other("Semantic errors found"));
        }

        info!("finished semantic analysis");
        info!("starting code generation");

        let mut generator: Box<dyn CodeGenerator> = match self.target {
            Target::Wasm => Box::new(WasmGenerator::new(&ast, &symbol_info)),
        };

        let text = generator.generate()?;

        info!("finished code generation");
        fs::write(out_path, &text)?;
        info!("created file: {}", out_path);

        // Also emit a binary `.wasm` (what browsers/Node load) and an `.abi.json` sidecar
        // describing extern imports and exports so the JS runtime can auto-marshal values.
        emit_wasm_and_abi(out_path, &text, ast.get_root())?;

        Ok(())
    }
}
