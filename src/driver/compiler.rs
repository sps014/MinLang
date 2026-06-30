use bumpalo::Bump;
use std::fs;
use std::io::Error;
use tracing::info;

use crate::codegen::wasm::WasmGenerator;
use crate::codegen::CodeGenerator;
use crate::driver::abi::emit_wasm_and_abi;
use crate::driver::diagnostics::DiagnosticBag;
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
    /// When `true`, codegen emits allocator instrumentation so the `Debug.live_objects()` /
    /// `Debug.total_allocations()` probes report real values. Off by default (release builds pay
    /// no per-allocation cost); enabled via the CLI `--debug` flag or [`Compiler::with_debug_alloc`].
    debug_alloc: bool,
}

impl Compiler {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            debug_alloc: false,
        }
    }

    /// Builder: enable allocator instrumentation for this compilation.
    pub fn with_debug_alloc(mut self, on: bool) -> Self {
        self.debug_alloc = on;
        self
    }

    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), Error> {
        info!("starting parsing and multi-file resolution");
        let mut acc = crate::driver::source_manager::ProgramAccumulator::default();

        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);

        parse_file_recursive(main_file_path, &mut acc, &arena, &mut diagnostics)?;

        // The standard collections (List<T>, Map<K, V>) are embedded in the compiler and merged
        // into every program as a prelude. They are generic templates, so they emit no code unless
        // the program actually instantiates them.
        merge_prelude(
            &arena,
            &mut acc.all_functions,
            &mut acc.all_structs,
            &mut acc.all_enums,
            &mut acc.all_extends,
            &mut diagnostics,
            &mut acc.file_contents,
        )?;

        // Auto-derive `to_json`/`from_json` converters for every `@json` class (must run after
        // all classes are collected so `@json` field cross-references resolve).
        generate_json_derives(
            &arena,
            &acc.all_structs,
            &acc.all_enums,
            &mut acc.all_extends,
            &mut diagnostics,
            &mut acc.file_contents,
        )?;

        if diagnostics.has_errors() {
            crate::driver::diagnostics::render(&diagnostics, &acc.file_contents);
            return Err(Error::other("Syntax errors found during parsing"));
        }

        let combined_program = ProgramNode::new(
            vec![],
            acc.all_structs,
            acc.all_functions,
            acc.all_enums,
            acc.all_extends,
            acc.all_globals,
        );
        let ast = SyntaxTree::new(combined_program);

        info!("finished parsing");
        info!("starting semantic analysis");

        let mut analyzer = Analyzer::new(&ast, &arena);
        let symbol_info = match analyzer.analyze(&mut diagnostics) {
            Ok(info) => info,
            Err(_) => {
                crate::driver::diagnostics::render(&diagnostics, &acc.file_contents);
                return Err(Error::other("Semantic errors found"));
            }
        };

        if diagnostics.has_errors() {
            crate::driver::diagnostics::render(&diagnostics, &acc.file_contents);
            return Err(Error::other("Semantic errors found"));
        }

        info!("finished semantic analysis");
        info!("starting code generation");

        let mut generator: Box<dyn CodeGenerator> = match self.target {
            Target::Wasm => {
                Box::new(WasmGenerator::new(&ast, &symbol_info).with_debug_alloc(self.debug_alloc))
            }
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
