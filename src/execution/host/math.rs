//! `Math.*` host functions, registered under the `env` module.

use wasmtime::*;

/// Registers the `Math.*` host functions on `linker`.
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
