//! Synchronous filesystem host functions (the `Dream` module behind `src/stdlib/io/file.dream`),
//! implemented over `std::fs`. Browser/Node hosts implement the same names in `runtime/dream.js`.

use std::fs;
use std::io::Write;
use std::path::Path;
use wasmtime::*;

use super::memory::{
    read_arg_bytes, read_arg_string, write_bytes_to_memory, write_string_to_memory,
};

/// Registers the synchronous filesystem host functions on `linker`. Shared by the CLI runner and
/// the E2E test harness so the native behavior can never drift.
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
        |mut caller: Caller<'_, ()>, path_ptr: i32, content_ptr: i32| -> i64 {
            let path = read_arg_string(&mut caller, path_ptr);
            let content = read_arg_string(&mut caller, content_ptr);
            match fs::write(&path, content.as_bytes()) {
                Ok(()) => content.len() as i64,
                Err(_) => -1,
            }
        },
    )?;

    linker.func_wrap(
        "Dream",
        "fileAppend",
        |mut caller: Caller<'_, ()>, path_ptr: i32, content_ptr: i32| -> i64 {
            let path = read_arg_string(&mut caller, path_ptr);
            let content = read_arg_string(&mut caller, content_ptr);
            let result = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| f.write_all(content.as_bytes()));
            match result {
                Ok(()) => content.len() as i64,
                Err(_) => -1,
            }
        },
    )?;

    // Binary I/O: a single bulk copy between the file and a Dream `byte[]`, no string round-trip.
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
        |mut caller: Caller<'_, ()>, path_ptr: i32, data_ptr: i32| -> i64 {
            let path = read_arg_string(&mut caller, path_ptr);
            let bytes = read_arg_bytes(&mut caller, data_ptr);
            match fs::write(&path, &bytes) {
                Ok(()) => bytes.len() as i64,
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
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> i64 {
            let path = read_arg_string(&mut caller, path_ptr);
            fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(-1)
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
