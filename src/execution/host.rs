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

/// Builds a `regex::Regex` from a pattern and a JS-style flags string ("i"/"m"/"s"; the global
/// "g" flag is handled per call site, and "u"/"y" have no Rust equivalent). Returns `None` on a
/// compile error (e.g. a pattern using lookaround/backreferences, which the `regex` crate rejects),
/// so callers can fall back to a safe default.
fn build_regex(pattern: &str, flags: &str) -> Option<regex::Regex> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .build()
        .ok()
}

/// Registers the synchronous regex host functions (the `Dream` module behind
/// `src/stdlib/regex.dream`) on `linker`, implemented with the `regex` crate. These mirror the JS
/// helpers in `runtime/dream.js` so `Regex.test`/`replace`/`match` behave the same on wasmtime,
/// Node, and the browser (for the common pattern subset the `regex` crate supports).
pub fn link_regex_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "Dream",
        "regexTest",
        |mut caller: Caller<'_, ()>, pattern_ptr: i32, flags_ptr: i32, input_ptr: i32| -> i32 {
            let pattern = read_arg_string(&mut caller, pattern_ptr);
            let flags = read_arg_string(&mut caller, flags_ptr);
            let input = read_arg_string(&mut caller, input_ptr);
            build_regex(&pattern, &flags).map_or(0, |re| re.is_match(&input) as i32)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "regexReplace",
        |mut caller: Caller<'_, ()>,
         pattern_ptr: i32,
         flags_ptr: i32,
         input_ptr: i32,
         replacement_ptr: i32|
         -> i32 {
            let pattern = read_arg_string(&mut caller, pattern_ptr);
            let flags = read_arg_string(&mut caller, flags_ptr);
            let input = read_arg_string(&mut caller, input_ptr);
            let replacement = read_arg_string(&mut caller, replacement_ptr);
            let out = match build_regex(&pattern, &flags) {
                Some(re) => {
                    if flags.contains('g') {
                        re.replace_all(&input, replacement.as_str()).into_owned()
                    } else {
                        re.replace(&input, replacement.as_str()).into_owned()
                    }
                }
                None => input.clone(),
            };
            write_string_to_memory(&mut caller, &out)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "regexMatchJoined",
        |mut caller: Caller<'_, ()>,
         pattern_ptr: i32,
         flags_ptr: i32,
         input_ptr: i32,
         sep_ptr: i32|
         -> i32 {
            let pattern = read_arg_string(&mut caller, pattern_ptr);
            let flags = read_arg_string(&mut caller, flags_ptr);
            let input = read_arg_string(&mut caller, input_ptr);
            let sep = read_arg_string(&mut caller, sep_ptr);
            let joined = match build_regex(&pattern, &flags) {
                Some(re) => {
                    if flags.contains('g') {
                        // Global: every full match (no capture groups), like JS `match` with `g`.
                        re.find_iter(&input)
                            .map(|m| m.as_str().to_string())
                            .collect::<Vec<_>>()
                            .join(&sep)
                    } else {
                        // Non-global: the first match plus its capture groups, like JS `match`
                        // without `g`. Missing optional groups render as "".
                        match re.captures(&input) {
                            Some(caps) => (0..caps.len())
                                .map(|i| caps.get(i).map_or("", |m| m.as_str()).to_string())
                                .collect::<Vec<_>>()
                                .join(&sep),
                            None => String::new(),
                        }
                    }
                }
                None => String::new(),
            };
            write_string_to_memory(&mut caller, &joined)
        },
    )?;

    Ok(())
}

/// Future heap-block sizing/kind, mirroring `codegen::wasm::async_support` (`F_SLOTS` = 56) and
/// `runtime/dream.js` (`FUTURE_KIND_HOST` = 1). A host future saves no locals, so its block is
/// exactly the fixed header region.
const FUTURE_SLOTS_SIZE: i32 = 56;
const FUTURE_KIND_HOST: i32 = 1;

/// Calls an exported function on the caller's module by name with the given typed signature.
fn call_export_2(caller: &mut Caller<'_, ()>, name: &str, a: i32, b: i32) {
    let func = caller
        .get_export(name)
        .and_then(Extern::into_func)
        .unwrap_or_else(|| panic!("module must export `{}`", name))
        .typed::<(i32, i32), ()>(&*caller)
        .unwrap_or_else(|_| panic!("unexpected `{}` signature", name));
    func.call(&mut *caller, (a, b))
        .unwrap_or_else(|_| panic!("`{}` call failed", name));
}

/// Bridges a synchronous (blocking) host result into Dream's async runtime, mirroring
/// `wrapAsyncImport` in `runtime/dream.js`: allocate a host `Future` via the module's exported
/// `__dream_new_future`, write `bytes` as a `char[]`, resolve the future via `__dream_resolve`, and
/// return the future pointer. The future is already settled when the awaiting task inspects it, so
/// the scheduler simply re-polls the waiter.
fn resolve_host_future_bytes(caller: &mut Caller<'_, ()>, bytes: &[u8]) -> i32 {
    let new_future = caller
        .get_export("__dream_new_future")
        .and_then(Extern::into_func)
        .expect("module must export `__dream_new_future`")
        .typed::<(i32, i32, i32), i32>(&*caller)
        .expect("unexpected `__dream_new_future` signature");
    let future = new_future
        .call(&mut *caller, (FUTURE_SLOTS_SIZE, -1, FUTURE_KIND_HOST))
        .expect("`__dream_new_future` call failed");
    let data_ptr = write_bytes_to_memory(caller, bytes);
    call_export_2(caller, "__dream_resolve", future, data_ptr);
    future
}

/// Performs one blocking HTTP request and serializes the whole response into the wire format shared
/// with `runtime/dream.js` (and parsed by `src/stdlib/http.dream`): an ASCII head ("<status>\n" plus
/// "Name: value\n" lines), a blank line, then the raw body bytes. `body` is sent verbatim unless the
/// verb is GET/HEAD or it is empty. Network/protocol errors come back as a status `0` response whose
/// body is the error text.
fn perform_http(method: &str, url: &str, headers_json: &str, body: Vec<u8>) -> Vec<u8> {
    let verb = method.to_uppercase();
    let http_method =
        reqwest::Method::from_bytes(verb.as_bytes()).unwrap_or(reqwest::Method::GET);

    let client = reqwest::blocking::Client::new();
    let mut builder = client.request(http_method, url);

    if !headers_json.is_empty() {
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(headers_json)
        {
            for (name, value) in map.iter() {
                if let Some(v) = value.as_str() {
                    builder = builder.header(name.as_str(), v);
                }
            }
        }
    }

    if !body.is_empty() && verb != "GET" && verb != "HEAD" {
        builder = builder.body(body);
    }

    match builder.send() {
        Ok(response) => {
            let status = response.status().as_u16();
            let mut head = format!("{}\n", status);
            for (name, value) in response.headers().iter() {
                if let Ok(v) = value.to_str() {
                    head.push_str(name.as_str());
                    head.push_str(": ");
                    head.push_str(v);
                    head.push('\n');
                }
            }
            head.push('\n'); // blank line separating head from body
            let mut out = head.into_bytes();
            match response.bytes() {
                Ok(body_bytes) => out.extend_from_slice(&body_bytes),
                Err(_) => {}
            }
            out
        }
        Err(e) => {
            let mut out = b"0\n\n".to_vec(); // status 0 = transport error; body is the message
            out.extend_from_slice(e.to_string().as_bytes());
            out
        }
    }
}

/// Registers the HTTP host functions (the `Dream` module behind `src/stdlib/http.dream`) on
/// `linker`. Each performs the whole request synchronously (blocking `reqwest`) and resolves a host
/// future with the serialized response, so the same `.dream` works under wasmtime, Node, and the
/// browser. `httpRequest` takes a text body; `httpRequestBytes` takes a binary `char[]` body.
pub fn link_http_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "Dream",
        "httpRequest",
        |mut caller: Caller<'_, ()>,
         url_ptr: i32,
         method_ptr: i32,
         headers_ptr: i32,
         body_ptr: i32|
         -> i32 {
            let url = read_arg_string(&mut caller, url_ptr);
            let method = read_arg_string(&mut caller, method_ptr);
            let headers = read_arg_string(&mut caller, headers_ptr);
            let body = read_arg_string(&mut caller, body_ptr).into_bytes();
            let response = perform_http(&method, &url, &headers, body);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "httpRequestBytes",
        |mut caller: Caller<'_, ()>,
         url_ptr: i32,
         method_ptr: i32,
         headers_ptr: i32,
         body_ptr: i32|
         -> i32 {
            let url = read_arg_string(&mut caller, url_ptr);
            let method = read_arg_string(&mut caller, method_ptr);
            let headers = read_arg_string(&mut caller, headers_ptr);
            let body = read_arg_bytes(&mut caller, body_ptr);
            let response = perform_http(&method, &url, &headers, body);
            resolve_host_future_bytes(&mut caller, &response)
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
