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

pub fn execute_wasm(wat_path: &str) -> Result<(), Box<dyn std::error::Error>> {
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

    linker.func_wrap("env", "print_double", |v: f64| {
        println!("{}", v);
    })?;

    linker.func_wrap("env", "print_string", |mut caller: Caller<'_, ()>, ptr: i32| {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let s = read_string_from_memory(&memory, &caller, ptr);
        print!("{}", s);
    })?;

    linker.func_wrap("env", "println", |mut caller: Caller<'_, ()>, ptr: i32| {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let s = read_string_from_memory(&memory, &caller, ptr);
        println!("{}", s);
    })?;

    linker.func_wrap("env", "sin", |v: f32| -> f32 { v.sin() })?;
    linker.func_wrap("env", "cos", |v: f32| -> f32 { v.cos() })?;
    linker.func_wrap("env", "abs", |v: f32| -> f32 { v.abs() })?;
    linker.func_wrap("env", "sqrt", |v: f32| -> f32 { v.sqrt() })?;
    linker.func_wrap("env", "strlen", |_: i32| -> i32 { 0 })?;
    linker.func_wrap("env", "debug_get_free_list_head", || -> i32 { 0 })?;

    let instance = linker.instantiate(&mut store, &module)?;
    
    if let Ok(main_func) = instance.get_typed_func::<(), ()>(&mut store, "main") {
        main_func.call(&mut store, ())?;
    } else {
        println!("No main function found in module");
    }

    Ok(())
}