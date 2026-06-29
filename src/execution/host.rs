//! Wasmtime host glue shared between the CLI runtime ([`super::wasm_runner`]) and the E2E test
//! harness (`tests/e2e_tests.rs`). Both link against the same `env` imports; only the output
//! sink differs (real stdout vs. a captured buffer), so the genuinely identical pieces -
//! string reads and the `Math.*` host functions - live here to avoid drift.

use wasmtime::*;

/// Reads a NUL-terminated UTF-8 string from `memory` starting at `ptr`.
pub fn read_string_from_memory(memory: &Memory, store: impl AsContext, ptr: i32) -> String {
    let data = memory.data(&store);
    let mut end = ptr as usize;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    String::from_utf8_lossy(&data[ptr as usize..end]).into_owned()
}

/// Registers the `Math.*` host functions on `linker` under the `env` module.
pub fn link_math_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap("env", "sin", |v: f64| -> f64 { v.sin() })?;
    linker.func_wrap("env", "cos", |v: f64| -> f64 { v.cos() })?;
    linker.func_wrap("env", "tan", |v: f64| -> f64 { v.tan() })?;
    linker.func_wrap("env", "abs", |v: f64| -> f64 { v.abs() })?;
    linker.func_wrap("env", "sqrt", |v: f64| -> f64 { v.sqrt() })?;
    linker.func_wrap("env", "pow", |base: f64, exp: f64| -> f64 { base.powf(exp) })?;
    linker.func_wrap("env", "floor", |v: f64| -> f64 { v.floor() })?;
    linker.func_wrap("env", "ceil", |v: f64| -> f64 { v.ceil() })?;
    linker.func_wrap("env", "round", |v: f64| -> f64 { v.round() })?;
    Ok(())
}
