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
use crate::lang::semantic_analysis::analyzer::Anaylzer;

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

    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), Error> {
        info!("starting parsing and multi-file resolution");
        let mut visited_files = HashSet::new();
        let mut all_functions = vec![];
        
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        
        self.parse_file_recursive(main_file_path, &mut visited_files, &mut all_functions, &arena, &mut diagnostics)?;
        
        if diagnostics.has_errors() {
            for diag in &diagnostics.diagnostics {
                error!("{}", diag.to_string());
            }
            return Err(Error::new(ErrorKind::Other, "Syntax errors found during parsing"));
        }

        let combined_program = ProgramNode::new(vec![], all_functions);
        let ast = SyntaxTree::new(combined_program);
        
        info!("finished parsing");
        info!("starting semantic analysis");
        
        let mut analyzer = Anaylzer::new(&ast);
        let symbol_info = match analyzer.analyze(&mut diagnostics) {
            Ok(info) => info,
            Err(_) => {
                for diag in &diagnostics.diagnostics {
                    error!("{}", diag.to_string());
                }
                return Err(Error::new(ErrorKind::Other, "Semantic errors found"));
            }
        };

        if diagnostics.has_errors() {
            for diag in &diagnostics.diagnostics {
                error!("{}", diag.to_string());
            }
            return Err(Error::new(ErrorKind::Other, "Semantic errors found"));
        }

        info!("finished semantic analysis");
        info!("starting code generation");
        
        let mut generator: Box<dyn CodeGenerator> = match self.target {
            Target::Wasm => Box::new(WasmGenerator::new(&ast, &symbol_info)),
        };
        
        let text = generator.generate()?;
        
        info!("finished code generation");
        fs::write(format!("{}", out_path), text)?;
        info!("created file: {}", out_path.clone());
        Ok(())
    }

    fn parse_file_recursive<'a>(
        &self,
        file_path: &String,
        visited: &mut HashSet<String>,
        all_functions: &mut Vec<crate::lang::code_analysis::syntax::nodes::FunctionNode<'a>>,
        arena: &'a Bump,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), Error> {
        let path = Path::new(file_path).canonicalize()?;
        let path_str = path.to_str().unwrap().to_string();
        
        if visited.contains(&path_str) {
            return Ok(()); // Already processed
        }
        visited.insert(path_str.clone());
        
        let mut file = File::open(&path)?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        
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
        let parent_dir = path.parent().unwrap();
        
        for import in &program.imports {
            let module_name = import.module_name.text.trim_matches('"');
            let mut import_path = parent_dir.join(module_name);
            if import_path.extension().is_none() {
                import_path.set_extension("ml");
            }
            
            let import_path_str = import_path.to_str().unwrap().to_string();
            if !import_path.exists() {
                diagnostics.report_error(format!("Imported file not found: {}", import_path_str), Some(import.module_name.position.clone()));
                continue;
            }
            
            self.parse_file_recursive(&import_path_str, visited, all_functions, arena, diagnostics)?;
        }
        
        all_functions.extend(program.functions.clone());
        
        Ok(())
    }
}
