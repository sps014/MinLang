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
        let mut file_contents = std::collections::HashMap::new();
        
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        
        self.parse_file_recursive(main_file_path, &mut visited_files, &mut all_functions, &mut all_structs, &arena, &mut diagnostics, &mut file_contents)?;
        
        if diagnostics.has_errors() {
            self.print_diagnostics(&diagnostics, &file_contents);
            return Err(Error::new(ErrorKind::Other, "Syntax errors found during parsing"));
        }

        let combined_program = ProgramNode::new(vec![], all_structs, all_functions);
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
        fs::write(out_path, text)?;
        info!("created file: {}", out_path);
        Ok(())
    }

    fn parse_file_recursive<'a>(
        &self,
        file_path: &String,
        visited: &mut HashSet<String>,
        all_functions: &mut Vec<crate::lang::code_analysis::syntax::nodes::FunctionNode<'a>>,
        all_structs: &mut Vec<crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
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
            
            self.parse_file_recursive(&import_path_str, visited, all_functions, all_structs, arena, diagnostics, file_contents)?;
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
        
        Ok(())
    }
}
