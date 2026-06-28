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

/// Registers the `Math.*` host functions on `linker` under the `env` module. The recognized set
/// is the intrinsic registry's [`crate::intrinsics::MATH_FUNCTIONS`]; the native implementation
/// for each is selected here.
pub fn link_math_functions(linker: &mut Linker<()>) -> Result<()> {
    for name in crate::intrinsics::MATH_FUNCTIONS {
        match name {
            "sin" => {
                linker.func_wrap("env", name, |v: f32| -> f32 { v.sin() })?;
            }
            "cos" => {
                linker.func_wrap("env", name, |v: f32| -> f32 { v.cos() })?;
            }
            "abs" => {
                linker.func_wrap("env", name, |v: f32| -> f32 { v.abs() })?;
            }
            "sqrt" => {
                linker.func_wrap("env", name, |v: f32| -> f32 { v.sqrt() })?;
            }
            other => unreachable!("no native implementation for Math.{}", other),
        }
    }
    Ok(())
}
