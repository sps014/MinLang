#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
mod lang;

use std::path::Path;
use tracing::{info, error};
use min_lang::lang::compiler::{Compiler, Target};
use min_lang::lang::execution::wasm_runner::execute_wasm;

fn main()
{
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    info!("MinLang Compiler Tools");
    info!("========================");

    let mut run_after_compile = false;
    let file_name = if args.len() == 3 && args[1] == "run" {
        run_after_compile = true;
        &args[2]
    } else if args.len() == 2 {
        &args[1]
    } else {
        error!("Expected a source file (*.ml) as argument");
        error!("Usage: {} [run] <file>", args[0]);
        error!(r"Example: {} run src/sample/test_arrays.ml", args[0]);
        return;
    };

    info!("Compiling file: {}", file_name);

    let compiler = Compiler::new(Target::Wasm);
    let out_path = get_path_from_file_path(file_name);

    match compiler.compile(file_name, &out_path)
    {
        Ok(_) => {
            info!("Compilation successful");
            
            if run_after_compile {
                info!("Executing via Wasmtime...");
                if let Err(e) = execute_wasm(&out_path) {
                    error!("Execution failed: {}", e);
                }
            }
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
