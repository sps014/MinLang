mod lang;

use std::path::Path;
use std::process::Command;
use tracing::{info, error};
use crate::lang::compiler::{Compiler, Target};

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
                let wasm_path = out_path.replace(".wat", ".wasm");
                info!("Executing wat2wasm...");
                let wat2wasm_status = Command::new("wat2wasm")
                    .arg(&out_path)
                    .arg("-o")
                    .arg(&wasm_path)
                    .status();
                    
                match wat2wasm_status {
                    Ok(status) if status.success() => {
                        info!("Executing node runner...");
                        let runner_path = Path::new(file_name).parent().unwrap().join("runner.js");
                        let node_status = Command::new("node")
                            .arg(runner_path)
                            .arg(&wasm_path)
                            .status();
                            
                        if let Err(e) = node_status {
                            error!("Failed to execute node: {}", e);
                        }
                    },
                    Ok(status) => error!("wat2wasm failed with status: {}", status),
                    Err(e) => error!("Failed to execute wat2wasm: {}", e),
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
