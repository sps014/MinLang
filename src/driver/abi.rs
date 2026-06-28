use std::fs;
use std::io::Error;
use std::path::Path;
use tracing::{info, error};

use crate::syntax::nodes::ProgramNode;

/// Emits a binary `.wasm` next to the `.wat`, plus an `.abi.json` describing the module's
/// extern imports (for JS interop marshaling) and exported functions.
pub(crate) fn emit_wasm_and_abi(wat_path: &str, wat_text: &str, program: &ProgramNode) -> Result<(), Error> {
    let base = Path::new(wat_path);

    let wasm_path = base.with_extension("wasm");
    match wat::parse_str(wat_text) {
        Ok(bytes) => {
            fs::write(&wasm_path, bytes)?;
            info!("created file: {}", wasm_path.display());
        }
        Err(e) => {
            // Non-fatal: the `.wat` is still valid output; just warn.
            error!("could not assemble binary wasm: {}", e);
        }
    }

    let abi_path = base.with_extension("abi.json");
    fs::write(&abi_path, build_abi_json(program))?;
    info!("created file: {}", abi_path.display());
    Ok(())
}

/// Escapes a string for embedding in a JSON document.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Builds the `.abi.json` describing extern imports and exported functions. The JS runtime uses
/// this to wrap user-supplied import implementations with the correct value marshaling.
pub(crate) fn build_abi_json(program: &ProgramNode) -> String {
    fn type_name(t: Option<&crate::syntax::nodes::Type>) -> String {
        match t {
            Some(t) => t.get_type(),
            None => "void".to_string(),
        }
    }

    let mut externs = Vec::new();
    for func in program.functions.iter() {
        if !func.is_extern { continue; }
        let module = func.import_module.clone().unwrap_or_else(|| "env".to_string());
        let field = func.import_name.clone().unwrap_or_else(|| func.name.text.clone());
        let params: Vec<String> = func.parameters.iter()
            .map(|p| format!("\"{}\"", json_escape(&p.type_.get_type())))
            .collect();
        externs.push(format!(
            "    {{ \"name\": \"{}\", \"module\": \"{}\", \"field\": \"{}\", \"params\": [{}], \"result\": \"{}\", \"async\": {} }}",
            json_escape(&func.name.text),
            json_escape(&module),
            json_escape(&field),
            params.join(", "),
            json_escape(&type_name(func.return_type.as_ref())),
            func.is_async,
        ));
    }

    let mut exports = Vec::new();
    for func in program.functions.iter() {
        if func.is_extern || func.generic_parameters.is_some() { continue; }
        if func.is_exported || func.name.text == "main" {
            exports.push(format!("\"{}\"", json_escape(&func.name.text)));
        }
    }

    format!(
        "{{\n  \"externs\": [\n{}\n  ],\n  \"exports\": [{}]\n}}\n",
        externs.join(",\n"),
        exports.join(", "),
    )
}
