#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
mod lang;

use std::path::Path;
use tracing::{info, error};
use min_lang::lang::compiler::{Compiler, Target};
use std::fs;
use wasmtime::*;

fn read_string_from_memory(memory: &Memory, store: impl AsContext, ptr: i32) -> String {
    let data = memory.data(&store);
    let mut end = ptr as usize;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    String::from_utf8_lossy(&data[ptr as usize..end]).into_owned()
}

fn execute_wasm(wat_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let wat_content = fs::read_to_string(wat_path)?;
    let wasm_bytes = wat::parse_str(&wat_content)?;

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes)?;
    
    let mut store = Store::new(&engine, ());
    let mut linker = Linker::new(&engine);

    linker.func_wrap("env", "print_int", |v: i32| {
        println!("{}", v);
    })?;

    linker.func_wrap("env", "print_float", |v: f32| {
        println!("{}", v);
    })?;

    linker.func_wrap("env", "print", |mut caller: Caller<'_, ()>, ptr: i32| {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let s = read_string_from_memory(&memory, &caller, ptr);
        print!("{}", s);
    })?;

    linker.func_wrap("env", "println", |mut caller: Caller<'_, ()>, ptr: i32| {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let s = read_string_from_memory(&memory, &caller, ptr);
        println!("{}", s);
    })?;

    linker.func_wrap("env", "concat_strings", |_: i32, _: i32| -> i32 {
        0 // Dummy implementation
    })?;

    linker.func_wrap("env", "sin", |v: f32| -> f32 { v.sin() })?;
    linker.func_wrap("env", "cos", |v: f32| -> f32 { v.cos() })?;
    linker.func_wrap("env", "abs", |v: f32| -> f32 { v.abs() })?;
    linker.func_wrap("env", "sqrt", |v: f32| -> f32 { v.sqrt() })?;
    linker.func_wrap("env", "strlen", |_: i32| -> i32 { 0 })?;
    linker.func_wrap("env", "malloc", |_: i32| -> i32 { 0 })?;
    linker.func_wrap("env", "free", |_: i32| {})?;

    let instance = linker.instantiate(&mut store, &module)?;
    
    if let Ok(main_func) = instance.get_typed_func::<(), ()>(&mut store, "main") {
        main_func.call(&mut store, ())?;
    } else {
        println!("No main function found in module");
    }

    Ok(())
}

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
