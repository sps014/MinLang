use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::Path;
use bumpalo::Bump;
use tracing::{info, error};

use crate::lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::code_analysis::syntax::nodes::ProgramNode;
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_generator::wasm::WasmGenerator;
use crate::lang::code_generator::CodeGenerator;
use crate::lang::diagnostics::DiagnosticBag;
use crate::lang::semantic_analysis::analyzer::Analyzer;

pub enum Target {
    Wasm,
}

pub struct Compiler {
    target: Target,
}

impl Compiler {
    pub fn new(target: Target) -> Self {
        Self { target }
    }

    fn print_diagnostics(&self, diagnostics: &DiagnosticBag, file_contents: &std::collections::HashMap<String, String>) {
        for diag in &diagnostics.diagnostics {
            error!("{}", diag.to_string());
            if let (Some(path), Some(span)) = (&diag.file_path, &diag.span) {
                if let Some(content) = file_contents.get(path) {
                    let lines: Vec<&str> = content.lines().collect();
                    if span.line_no > 0 && span.line_no <= lines.len() {
                        let line_text = lines[span.line_no - 1];
                        error!("  | {}", line_text);
                        let padding = " ".repeat(span.col_no.saturating_sub(1));
                        let squiggly_len = if span.end > span.start { span.end - span.start } else { 1 };
                        let squiggly = "^".repeat(squiggly_len);
                        error!("  | {}{}", padding, squiggly);
                    }
                }
            }
        }
    }

    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), Error> {
        info!("starting parsing and multi-file resolution");
        let mut visited_files = HashSet::new();
        let mut all_functions = vec![];
        let mut all_structs = vec![];
        let mut all_enums = vec![];
        let mut file_contents = std::collections::HashMap::new();
        
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        
        self.parse_file_recursive(main_file_path, &mut visited_files, &mut all_functions, &mut all_structs, &mut all_enums, &arena, &mut diagnostics, &mut file_contents)?;

        // The standard collections (List<T>, Map<K, V>) are embedded in the compiler and merged
        // into every program as a prelude. They are generic templates, so they emit no code unless
        // the program actually instantiates them.
        self.merge_prelude(&arena, &mut all_functions, &mut all_structs, &mut diagnostics, &mut file_contents)?;

        if diagnostics.has_errors() {
            self.print_diagnostics(&diagnostics, &file_contents);
            return Err(Error::new(ErrorKind::Other, "Syntax errors found during parsing"));
        }

        let combined_program = ProgramNode::new(vec![], all_structs, all_functions, all_enums);
        let ast = SyntaxTree::new(combined_program);
        
        info!("finished parsing");
        info!("starting semantic analysis");
        
        let mut analyzer = Analyzer::new(&ast, &arena);
        let symbol_info = match analyzer.analyze(&mut diagnostics) {
            Ok(info) => info,
            Err(_) => {
                self.print_diagnostics(&diagnostics, &file_contents);
                return Err(Error::new(ErrorKind::Other, "Semantic errors found"));
            }
        };

        if diagnostics.has_errors() {
            self.print_diagnostics(&diagnostics, &file_contents);
            return Err(Error::new(ErrorKind::Other, "Semantic errors found"));
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
        self.emit_wasm_and_abi(out_path, &text, ast.get_root())?;

        Ok(())
    }

    /// Emits a binary `.wasm` next to the `.wat`, plus an `.abi.json` describing the module's
    /// extern imports (for JS interop marshaling) and exported functions.
    fn emit_wasm_and_abi(&self, wat_path: &str, wat_text: &str, program: &ProgramNode) -> Result<(), Error> {
        let base = Path::new(wat_path);

        let wasm_path = base.with_extension("wasm");
        match wat::parse_str(wat_text) {
            Ok(bytes) => {
                fs::write(&wasm_path, bytes)?;
                info!("created file: {}", wasm_path.display());
            }
            Err(e) => {
                // Non-fatal: the `.wat` is still valid output; just warn.
                error!("could not assemble binary wasm: {}", e);
            }
        }

        let abi_path = base.with_extension("abi.json");
        fs::write(&abi_path, build_abi_json(program))?;
        info!("created file: {}", abi_path.display());
        Ok(())
    }

    /// Parses the embedded standard-collections prelude and merges its declarations into the
    /// program. Uses the same arena as the user's files so all AST nodes share a lifetime.
    fn merge_prelude<'a>(
        &self,
        arena: &'a Bump,
        all_functions: &mut Vec<crate::lang::code_analysis::syntax::nodes::FunctionNode<'a>>,
        all_structs: &mut Vec<crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
        diagnostics: &mut DiagnosticBag,
        file_contents: &mut std::collections::HashMap<String, String>,
    ) -> Result<(), Error> {
        // Each standard-collection type lives in its own prelude file.
        const PRELUDE_FILES: [(&str, &str); 2] = [
            ("<std>/list.dream", include_str!("stdlib/list.dream")),
            ("<std>/map.dream", include_str!("stdlib/map.dream")),
        ];

        for (prelude_name, prelude_src) in PRELUDE_FILES {
            let prelude_name = prelude_name.to_string();
            file_contents.insert(prelude_name.clone(), prelude_src.to_string());

            let mut prelude_diagnostics = DiagnosticBag::new(Some(prelude_name.clone()));
            let lexer = Lexer::new(prelude_src.to_string());
            let mut parser = Parser::new(lexer, arena, &mut prelude_diagnostics);
            let ast = match parser.parse() {
                Ok(ast) => ast,
                Err(e) => {
                    diagnostics.extend(&prelude_diagnostics);
                    return Err(e);
                }
            };
            diagnostics.extend(&prelude_diagnostics);

            let program = ast.get_root();
            let file_tag: std::rc::Rc<str> = std::rc::Rc::from(prelude_name.as_str());
            for function in program.functions.iter().cloned() {
                let mut function = function;
                function.file_path = Some(file_tag.clone());
                all_functions.push(function);
            }
            for struct_decl in program.structs.iter().cloned() {
                let mut struct_decl = struct_decl;
                struct_decl.file_path = Some(file_tag.clone());
                for method in struct_decl.methods.iter_mut() {
                    method.file_path = Some(file_tag.clone());
                }
                all_structs.push(struct_decl);
            }
        }

        Ok(())
    }

    fn parse_file_recursive<'a>(
        &self,
        file_path: &String,
        visited: &mut HashSet<String>,
        all_functions: &mut Vec<crate::lang::code_analysis::syntax::nodes::FunctionNode<'a>>,
        all_structs: &mut Vec<crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
        all_enums: &mut Vec<crate::lang::code_analysis::syntax::nodes::EnumDeclarationNode>,
        arena: &'a Bump,
        diagnostics: &mut DiagnosticBag,
        file_contents: &mut std::collections::HashMap<String, String>,
    ) -> Result<(), Error> {
        let path = Path::new(file_path).canonicalize()?;
        let path_str = path.to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, format!("Non-UTF-8 file path: {:?}", path)))?
            .to_string();

        if visited.contains(&path_str) {
            return Ok(()); // Already processed
        }
        visited.insert(path_str.clone());
        
        let mut file = File::open(&path)?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;

        // `print` (along with `to_string`/`hash_code`) is now a compiler builtin resolved during
        // code generation via the object protocol, so no source injection is needed.

        file_contents.insert(path_str.clone(), text.clone());
        
        let mut file_diagnostics = DiagnosticBag::new(Some(path_str.clone()));
        
        let lexer = Lexer::new(text);
        let mut parser = Parser::new(lexer, arena, &mut file_diagnostics);
        
        let ast = match parser.parse() {
            Ok(ast) => ast,
            Err(e) => {
                diagnostics.extend(&file_diagnostics);
                return Err(e);
            }
        };
        
        diagnostics.extend(&file_diagnostics);
        
        let program = ast.get_root();
        let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
        
        for import in &program.imports {
            let module_name = import.module_name.text.trim_matches('"');
            let mut import_path = parent_dir.join(module_name);
            if import_path.extension().is_none() {
                import_path.set_extension("ml");
            }
            
            let import_path_str = match import_path.to_str() {
                Some(s) => s.to_string(),
                None => {
                    diagnostics.report_error(format!("Non-UTF-8 import path: {:?}", import_path), Some(import.module_name.position.clone()));
                    continue;
                }
            };
            if !import_path.exists() {
                diagnostics.report_error(format!("Imported file not found: {}", import_path_str), Some(import.module_name.position.clone()));
                continue;
            }
            
            self.parse_file_recursive(&import_path_str, visited, all_functions, all_structs, all_enums, arena, diagnostics, file_contents)?;
        }
        
        // Tag every declaration with its source file so semantic diagnostics (which run on the
        // merged program) can report the correct file name.
        let file_tag: std::rc::Rc<str> = std::rc::Rc::from(path_str.as_str());
        for function in program.functions.iter().cloned() {
            let mut function = function;
            function.file_path = Some(file_tag.clone());
            all_functions.push(function);
        }
        for struct_decl in program.structs.iter().cloned() {
            let mut struct_decl = struct_decl;
            struct_decl.file_path = Some(file_tag.clone());
            for method in struct_decl.methods.iter_mut() {
                method.file_path = Some(file_tag.clone());
            }
            all_structs.push(struct_decl);
        }
        for enum_decl in program.enums.iter().cloned() {
            all_enums.push(enum_decl);
        }
        
        Ok(())
    }
}

/// Escapes a string for embedding in a JSON document.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Builds the `.abi.json` describing extern imports and exported functions. The JS runtime uses
/// this to wrap user-supplied import implementations with the correct value marshaling.
fn build_abi_json(program: &ProgramNode) -> String {
    fn type_name(t: Option<&crate::lang::code_analysis::syntax::nodes::Type>) -> String {
        match t {
            Some(t) => t.get_type(),
            None => "void".to_string(),
        }
    }

    let mut externs = Vec::new();
    for func in program.functions.iter() {
        if !func.is_extern { continue; }
        let module = func.import_module.clone().unwrap_or_else(|| "env".to_string());
        let field = func.import_name.clone().unwrap_or_else(|| func.name.text.clone());
        let params: Vec<String> = func.parameters.iter()
            .map(|p| format!("\"{}\"", json_escape(&p.type_.get_type())))
            .collect();
        externs.push(format!(
            "    {{ \"name\": \"{}\", \"module\": \"{}\", \"field\": \"{}\", \"params\": [{}], \"result\": \"{}\" }}",
            json_escape(&func.name.text),
            json_escape(&module),
            json_escape(&field),
            params.join(", "),
            json_escape(&type_name(func.return_type.as_ref())),
        ));
    }

    let mut exports = Vec::new();
    for func in program.functions.iter() {
        if func.is_extern || func.generic_parameters.is_some() { continue; }
        if func.is_exported || func.name.text == "main" {
            exports.push(format!("\"{}\"", json_escape(&func.name.text)));
        }
    }

    format!(
        "{{\n  \"externs\": [\n{}\n  ],\n  \"exports\": [{}]\n}}\n",
        externs.join(",\n"),
        exports.join(", "),
    )
}
