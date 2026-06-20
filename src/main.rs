mod lang;

use std::path::Path;
use tracing::{info, error};
use crate::lang::compiler::{Compiler, Target};

fn main()
{
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    info!("MinLang Compiler Tools");
    info!("========================");

    if args.len() != 2 {
        error!("Expected a source file (*.ml) as argument");
        error!("Usage: {} <file>", args[0]);
        error!(r"Example: ./min_lang \src\sample\main.ml");
        return;
    }
    let file_name = &args[1];

    info!("Compiling file: {}", file_name);

    let compiler = Compiler::new(Target::Wasm);
    let out_path = get_path_from_file_path(file_name);

    match compiler.compile(file_name, &out_path)
    {
        Ok(_) => {
            info!("Compilation successful");
        },
        Err(e) => {
            error!("Compilation failed: {}", e.to_string());
        }
    }
}

fn get_path_from_file_path(file_path:&String)->String
{
    let path=Path::new(file_path);
    let file_name_without_ext=path.file_stem().unwrap().to_str().unwrap();
    let result=path.parent().unwrap().join(format!("{}.wat",file_name_without_ext));
    return result.to_str().unwrap().to_string();
}
