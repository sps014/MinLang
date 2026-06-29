//! Wasmtime host glue shared between the CLI runtime ([`super::wasm_runner`]) and the E2E test
//! harness (`tests/e2e_tests.rs`). Both link against the same `env` imports; only the output
//! sink differs (real stdout vs. a captured buffer), so the genuinely identical pieces -
//! string reads and the `Math.*` host functions - live here to avoid drift.

use std::fs;
use std::io::Write;
use std::path::Path;
use wasmtime::*;

/// The heap-block tag codegen uses for strings (mirrors `codegen::wasm::object::TAG_STRING`).
/// A host that allocates a string into linear memory must tag the block with this so the runtime
/// treats it as a string.
const TAG_STRING: i32 = 5;

/// The heap-block tag codegen uses for arrays (mirrors `codegen::wasm::object::TAG_ARRAY`). A
/// `char[]` (byte array) is laid out as `[count: i32][bytes...]` at the data pointer.
const TAG_ARRAY: i32 = 6;

/// Reads a NUL-terminated UTF-8 string from `memory` starting at `ptr`.
pub fn read_string_from_memory(memory: &Memory, store: impl AsContext, ptr: i32) -> String {
    let data = memory.data(&store);
    let mut end = ptr as usize;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    String::from_utf8_lossy(&data[ptr as usize..end]).into_owned()
}

/// Reads the caller's exported `memory` and returns the NUL-terminated string at `ptr`.
fn read_arg_string(caller: &mut Caller<'_, ()>, ptr: i32) -> String {
    let memory = caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .expect("module must export `memory`");
    read_string_from_memory(&memory, &*caller, ptr)
}

/// Allocates `s` as a Dream `string` inside the module's linear memory by calling its exported
/// `malloc`, copying the UTF-8 bytes, and NUL-terminating. Returns the data pointer (mirrors
/// `DreamInstance.writeString` in `runtime/dream.js`). Used by host functions that return strings.
pub fn write_string_to_memory(caller: &mut Caller<'_, ()>, s: &str) -> i32 {
    let malloc = caller
        .get_export("malloc")
        .and_then(Extern::into_func)
        .expect("module must export `malloc`")
        .typed::<(i32, i32), i32>(&*caller)
        .expect("unexpected `malloc` signature");
    let bytes = s.as_bytes();
    let ptr = malloc
        .call(&mut *caller, (bytes.len() as i32 + 1, TAG_STRING))
        .expect("malloc call failed");
    let memory = caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .expect("module must export `memory`");
    let start = ptr as usize;
    let data = memory.data_mut(&mut *caller);
    data[start..start + bytes.len()].copy_from_slice(bytes);
    data[start + bytes.len()] = 0;
    ptr
}

/// Reads a Dream `char[]` (byte array) at data pointer `ptr` into a `Vec<u8>` with a single bulk
/// copy. Layout: `[count: i32][bytes...]` (char elements are 1 byte). No string round-trip, so
/// this is binary-safe.
fn read_arg_bytes(caller: &mut Caller<'_, ()>, ptr: i32) -> Vec<u8> {
    let memory = caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .expect("module must export `memory`");
    let data = memory.data(&*caller);
    let base = ptr as usize;
    let count =
        i32::from_le_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]) as usize;
    let start = base + 4;
    data[start..start + count].to_vec()
}

/// Allocates a Dream `char[]` (byte array) holding `bytes` via the module's exported `malloc`,
/// with a single bulk copy. Returns the array data pointer. Mirrors `DreamInstance.writeArray`
/// in `runtime/dream.js`.
pub fn write_bytes_to_memory(caller: &mut Caller<'_, ()>, bytes: &[u8]) -> i32 {
    let malloc = caller
        .get_export("malloc")
        .and_then(Extern::into_func)
        .expect("module must export `malloc`")
        .typed::<(i32, i32), i32>(&*caller)
        .expect("unexpected `malloc` signature");
    let count = bytes.len() as i32;
    let ptr = malloc
        .call(&mut *caller, (4 + count, TAG_ARRAY))
        .expect("malloc call failed");
    let memory = caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .expect("module must export `memory`");
    let base = ptr as usize;
    let data = memory.data_mut(&mut *caller);
    data[base..base + 4].copy_from_slice(&count.to_le_bytes());
    data[base + 4..base + 4 + bytes.len()].copy_from_slice(bytes);
    ptr
}

/// Registers the synchronous filesystem host functions (the `Dream` module behind
/// `src/stdlib/file.dream`) on `linker`. Shared by the CLI runner and the E2E test harness so the
/// native behavior can never drift. Browser hosts implement the same names in `runtime/dream.js`.
pub fn link_file_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "Dream",
        "fileRead",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            let content = fs::read_to_string(&path).unwrap_or_default();
            write_string_to_memory(&mut caller, &content)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileWrite",
        |mut caller: Caller<'_, ()>, path_ptr: i32, content_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            let content = read_arg_string(&mut caller, content_ptr);
            match fs::write(&path, content.as_bytes()) {
                Ok(()) => content.len() as i32,
                Err(_) => -1,
            }
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileAppend",
        |mut caller: Caller<'_, ()>, path_ptr: i32, content_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            let content = read_arg_string(&mut caller, content_ptr);
            let result = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| f.write_all(content.as_bytes()));
            match result {
                Ok(()) => content.len() as i32,
                Err(_) => -1,
            }
        },
    )?;

    // Binary I/O: a single bulk copy between the file and a Dream `char[]`, no string round-trip.
    linker.func_wrap(
        "Dream",
        "fileReadBytes",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            let bytes = fs::read(&path).unwrap_or_default();
            write_bytes_to_memory(&mut caller, &bytes)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileWriteBytes",
        |mut caller: Caller<'_, ()>, path_ptr: i32, data_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            let bytes = read_arg_bytes(&mut caller, data_ptr);
            match fs::write(&path, &bytes) {
                Ok(()) => bytes.len() as i32,
                Err(_) => -1,
            }
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileExists",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            Path::new(&path).exists() as i32
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileDelete",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            fs::remove_file(&path).is_ok() as i32
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileSize",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            fs::metadata(&path).map(|m| m.len() as i32).unwrap_or(-1)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileIsDir",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            Path::new(&path).is_dir() as i32
        },
    )?;

    linker.func_wrap(
        "Dream",
        "dirList",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i32 {
            let path = read_arg_string(&mut caller, path_ptr);
            let joined = match fs::read_dir(&path) {
                Ok(entries) => {
                    let mut names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect();
                    names.sort();
                    names.join("\n")
                }
                Err(_) => String::new(),
            };
            write_string_to_memory(&mut caller, &joined)
        },
    )?;

    Ok(())
}

/// Registers the `Math.*` host functions on `linker` under the `env` module.
pub fn link_math_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap("env", "sin", |v: f64| -> f64 { v.sin() })?;
    linker.func_wrap("env", "cos", |v: f64| -> f64 { v.cos() })?;
    linker.func_wrap("env", "tan", |v: f64| -> f64 { v.tan() })?;
    linker.func_wrap("env", "abs", |v: f64| -> f64 { v.abs() })?;
    linker.func_wrap("env", "sqrt", |v: f64| -> f64 { v.sqrt() })?;
    linker.func_wrap("env", "pow", |base: f64, exp: f64| -> f64 {
        base.powf(exp)
    })?;
    linker.func_wrap("env", "floor", |v: f64| -> f64 { v.floor() })?;
    linker.func_wrap("env", "ceil", |v: f64| -> f64 { v.ceil() })?;
    linker.func_wrap("env", "round", |v: f64| -> f64 { v.round() })?;
    Ok(())
}
