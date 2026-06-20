mod lang;

use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::Path;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::code_analysis::syntax::syntax_node::ProgramNode;
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_generator::wasm_generator::WasmGenerator;
use crate::lang::semantic_analysis::analyzer::Anaylzer;


fn main()
{
    let args: Vec<String> = std::env::args().collect();

    print_blue("MinLang Compiler Tools".to_string());
    print_info("========================".to_string());

    if args.len() != 2 {
        print_error("Expected a source file (*.ml) as argument".to_string());
        print_error(format!("Usage: {} <file>", args[0]));
        print_error(r"Example: ./min_lang \src\sample\main.ml".to_string());
        return;
    }
    let file_name = &args[1];

    print_warning(format!("Compiling file: {}", file_name));

    let mut file= File::open(file_name).expect("Error");
    let mut text=String::new();
    file.read_to_string(&mut text).unwrap();

    match cli_process(&file_name.clone(),&get_path_from_file_path(&file_name.clone()))
    {
        Ok(_) => {
            print_info("Compilation successful".to_string());
        },
        Err(e) => {
            print_error(format!("Compilation failed: {}", e.to_string()));
        }
    }
}

fn cli_process(main_file_path:&String, out_path:&String)->Result<(),Error>
{
    print_warning(format!("starting parsing and multi-file resolution"));
    let mut visited_files = HashSet::new();
    let mut all_functions = vec![];
    
    parse_file_recursive(main_file_path, &mut visited_files, &mut all_functions)?;
    
    let combined_program = ProgramNode::new(vec![], all_functions);
    let ast = SyntaxTree::new(combined_program);
    
    print_info("finished parsing".to_string());
    print_warning(format!("starting semantic analysis"));
    let mut analyzer=Anaylzer::new(&ast);
    let symbol_info=analyzer.analyze()?;
    print_info("finished semantic analysis".to_string());
    print_warning(format!("starting code generation (.wat file)"));
    let mut generator=WasmGenerator::new(&ast,&symbol_info);
    let text=generator.build()?;
    print_blue("finished code generation".to_string());
    fs::write(format!("{}",out_path),text.to_string())?;
    print_info(format!("created file: {}", out_path.clone()));
    Ok(())
}

fn parse_file_recursive(file_path: &String, visited: &mut HashSet<String>, all_functions: &mut Vec<crate::lang::code_analysis::syntax::syntax_node::FunctionNode>) -> Result<(), Error> {
    let path = Path::new(file_path).canonicalize()?;
    let path_str = path.to_str().unwrap().to_string();
    
    if visited.contains(&path_str) {
        return Ok(()); // Already processed
    }
    visited.insert(path_str.clone());
    
    let mut file = File::open(&path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    
    let lexer = Lexer::new(text);
    let mut parser = Parser::new(lexer);
    let ast = parser.parse()?;
    let program = ast.get_root();
    
    let parent_dir = path.parent().unwrap();
    
    for import in program.imports {
        // Strip quotes from import module name
        let module_name = import.module_name.text.trim_matches('"');
        let mut import_path = parent_dir.join(module_name);
        if import_path.extension().is_none() {
            import_path.set_extension("ml");
        }
        
        let import_path_str = import_path.to_str().unwrap().to_string();
        if !import_path.exists() {
            return Err(Error::new(ErrorKind::NotFound, format!("Imported file not found: {}", import_path_str)));
        }
        
        parse_file_recursive(&import_path_str, visited, all_functions)?;
    }
    
    all_functions.extend(program.functions);
    
    Ok(())
}

fn print_error(error: String) {
    //print error in red
    println!("\x1b[31m{}\x1b[0m", error);
}
fn print_warning(warning: String) {
    //print warning in yellow
    println!("\x1b[33m{}\x1b[0m", warning);
}
fn print_blue(text: String) {
    //print text in blue
    println!("\x1b[34m{}\x1b[0m", text);
}

fn print_info(info: String) {
    //print info in green
    println!("\x1b[32m{}\x1b[0m", info);
}

fn get_path_from_file_path(file_path:&String)->String
{
    let path=Path::new(file_path);
    let file_name_without_ext=path.file_stem().unwrap().to_str().unwrap();
    let result=path.parent().unwrap().join(format!("{}.wat",file_name_without_ext));
    return result.to_str().unwrap().to_string();
}
