mod lang;

use std::fs;
use std::fs::File;
use std::io::{Error, Read};
use std::path::Path;
use std::process::Command;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
//use crate::lang::code_generator::cpp_generator::CppGenerator;
use crate::lang::code_generator::wasm_generator::WasmGenerator;
use crate::lang::semantic_analysis::analyzer::Anaylzer;


fn main()
{
    let args: Vec<String> = std::env::args().collect();

    print_blue("MinLang Compiler Tools".to_string());
    print_info("========================".to_string());

    // if args.len() != 2 {
    //     print_error("Expected a source file (*.ml) as argument".to_string());
    //     print_error(format!("Usage: {} <file>", args[0]));
    //     print_error(r"Example: ./min_lang \src\sample\main.ml".to_string());
    //     return;
    // }
    //let file_name = &args[1];
    let file_name = r"D:\Projects\MinLang\src\sample\basic_sum.ml".to_string();


    print_warning(format!("Compiling file: {}", file_name));

    let mut file= File::open(&file_name).expect("Error");
    let mut text=String::new();
    file.read_to_string(&mut text).unwrap();

    match cli_process(&text,&get_path_from_file_path(&file_name.clone()))
    {
        Ok(_) => {
            print_info("Compilation successful".to_string());
        },
        Err(e) => {
            print_error(format!("Compilation failed: {}", e.to_string()));
        }
    }
}

fn cli_process(input:&String,path:&String)->Result<(),Error>
{
    print_warning(format!("starting parsing"));
    let lexer = Lexer::new(input.clone());
    let mut parser=Parser::new(lexer);
    let ast=parser.parse()?;
    print_info("finished parsing".to_string());
    print_warning(format!("starting semantic analysis"));
    let mut analyzer=Anaylzer::new(&ast);
    let symbol_info=analyzer.analyze()?;
    print_info("finished semantic analysis".to_string());
    print_warning(format!("starting code generation (.wat file)"));

    let mut generator=WasmGenerator::new(&ast,&symbol_info);
    // let mut generator=CppGenerator::new(&ast,&symbol_info);
     let text=generator.build()?;
    generate_index_js(&path);
    generate_index_html(&path);
    wat_2_wasm(&path);
    print_blue("finished code generation".to_string());
    fs::write(format!("{}",path),text.to_string())?;
    print_info(format!("created file: {}", path.clone()));

    print_blue(text.to_string());
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
fn generate_index_js(file_name:&String)
{
    let path=Path::new(file_name);
    let result=path.parent().unwrap().join("index.js");
    let file_name_without_ext=path.file_stem().unwrap().to_str().unwrap();

    let content=
        format!(
            r#"var memory = new WebAssembly.Memory({{initial:1}});

var importObject = {{ js: {{ mem: memory }} }};
let wasm_obj;
WebAssembly.instantiateStreaming(fetch('{}.wasm'), importObject)
  .then(obj => {{
    wasm_obj=obj.instance.exports;
  }});"#,file_name_without_ext
        );
    fs::write(result,content);
}

fn generate_index_html(file_name:&String)
{
    let path=Path::new(file_name);
    let result=path.parent().unwrap().join("index.html");
    let content =
        format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
	<meta charset="UTF-8">
	<meta http-equiv="X-UA-Compatible" content="IE=edge">
	<meta name="viewport" content="width=device-width, initial-scale=1.0">
	<title>Wasm</title>
	<script src="index.js"></script>
</head>
<body>

</body>
</html>
        "#);
    fs::write(result,content);

}

fn wat_2_wasm(file_name:&String)
{
    let path = Path::new(file_name);
    let file_name_without_ext=path.file_stem().unwrap().to_str().unwrap();
    let result=path.parent().unwrap().join(format!("{}",file_name_without_ext));
    let process=Command::new("wat2wasm")
        .args(["/C",
        format!("\"{}.wat\"",result.to_str().unwrap()).as_str(),
        format!("\"-o {}.wasm\"",result.to_str().unwrap()).as_str()])
        .output()
        .expect("failed to execute process, make sure wat2wasm is installed (part of WABT tool) and it's path is set in env variable");
    print_error(format!("{}",String::from_utf8_lossy(&process.stderr)));
}