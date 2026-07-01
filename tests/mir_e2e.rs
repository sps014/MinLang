//! Step-D coverage gate for the new MIR backend.
//!
//! Compiles every `tests/cases/*.dream` through the **real driver front-end** (prelude, json-derive,
//! multi-file resolution, analysis) but with code generation routed to the HIR → MIR → WAT backend
//! (`Compiler::with_mir(true)`), runs the result under `wasmtime`, and compares to the `.expected`
//! output. Cases the MIR backend does not yet cover are listed in `XFAIL` with the reason.
//!
//! The assertion is a ratchet: every case **not** in `XFAIL` must pass, so coverage can only grow —
//! removing an entry from `XFAIL` (as the backend gains a feature) is the unit of progress, and any
//! regression that breaks a previously-passing case fails the suite. When `XFAIL` is empty the driver
//! default can flip to the MIR backend and the legacy `WasmGenerator` can be deleted (Step D).

use dream::driver::compiler::{Compiler, Target};
use dream::execution::host::{
    link_file_functions, link_http_functions, link_math_functions, link_regex_functions,
    read_string_from_memory,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use wasmtime::*;

/// Cases the MIR backend cannot yet compile+run correctly, each with the missing capability. The
/// categories map directly to remaining Step-5 (runtime-integration) and analyzer HIR-coverage work:
///
/// * `main dropped` — `main`'s body uses a construct the analyzer does not yet lower to HIR (so the
///   whole function is skipped and the module has no `main` export): strings/interpolation, unions &
///   the object protocol, `do/while`, labeled loops, overload resolution, top-level statements, etc.
/// * `callee unresolved` — a reachable call/method/generic instance is not emitted, so it falls back
///   to the `$def{N}` placeholder (monomorphized methods and non-`main` generic bodies).
/// * `constructor/layout` — a `new` reaches the `$def{N}_constructor` fallback because the struct's
///   layout is not registered for that (generic) instance.
/// * `codegen bug` — compiles and runs but the output is wrong, or `main` fails WASM validation:
///   real correctness gaps in already-covered paths, to be fixed next.
///
/// Removing an entry (as coverage lands) is the unit of progress; when this list is empty the driver
/// default flips to the MIR backend and the legacy `WasmGenerator` is deleted (Step D).
const XFAIL: &[(&str, &str)] = &[
    // main dropped: unsupported construct in main's body.
    ("async_basic", "main dropped: async entry lowering"),
    ("async_combinators", "main dropped: async entry lowering"),
    ("concat", "main dropped: string concat"),
    ("do_while", "main dropped: do/while"),
    ("empty_array", "main dropped: empty array literal"),
    ("enum_name", "main dropped: enum name()"),
    ("gc_complete", "main dropped: Debug probes"),
    ("json_deep_nesting", "main dropped: json"),
    ("json_derive", "main dropped: json"),
    ("json_nullable", "main dropped: json"),
    ("json_property_name", "main dropped: json"),
    ("labeled_loops", "main dropped: labeled loops"),
    ("new_integer_types", "main dropped: wide-integer literals"),
    ("object_basics", "main dropped: object protocol"),
    ("object_protocol", "main dropped: object protocol"),
    ("overload_functions", "main dropped: overload resolution"),
    ("overload_methods", "main dropped: overload resolution"),
    ("string_interpolation", "main dropped: string interpolation"),
    ("strings", "main dropped: string methods"),
    ("union_hash_code", "main dropped: unions"),
    ("union_json", "main dropped: unions + json"),
    ("union_match", "main dropped: union match"),
    ("union_nested", "main dropped: nested unions"),
    ("union_to_string", "main dropped: union to_string"),
    ("main_args", "main signature: main(args) not the () entry shape"),
    // callee unresolved: reachable call/method/generic instance not emitted ($def{N}).
    ("async_method", "callee unresolved: async method"),
    ("async_ref_params", "callee unresolved: async method"),
    ("file_io", "callee unresolved: File intrinsics"),
    ("generics", "callee unresolved: generic instance body"),
    ("json_parse", "callee unresolved: json"),
    ("json_roundtrip", "callee unresolved: json"),
    // constructor/layout: new reaches $def{N}_constructor fallback.
    ("collections_growth", "constructor/layout: generic collection"),
    ("json_pretty", "constructor/layout: json"),
    ("list_basics", "constructor/layout: List<T>"),
    ("map_basics", "constructor/layout: Map<K,V>"),
    ("struct_methods", "constructor/layout"),
    // codegen bug: compiles/runs but output wrong, or main fails WASM validation.
];

#[derive(Clone)]
struct TestEnv {
    output: Arc<Mutex<String>>,
}

impl TestEnv {
    fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(String::new())),
        }
    }
    fn print(&self, s: &str) {
        self.output.lock().unwrap().push_str(s);
    }
}

/// Compiles one case through the MIR backend and runs it, returning `Ok(actual_output)` or an error
/// describing the failure stage (compile / assemble / instantiate / execute).
fn compile_and_run_mir(dream_file: &Path) -> Result<String, String> {
    let wat_path = dream_file.with_extension("mir.wat");
    let dream_str = dream_file.to_str().unwrap().to_string();
    let wat_str = wat_path.to_str().unwrap().to_string();

    let compiler = Compiler::new(Target::Wasm).with_mir(true).with_debug_alloc(true);
    compiler
        .compile(&dream_str, &wat_str)
        .map_err(|e| format!("compile: {e:?}"))?;

    let wat = fs::read_to_string(&wat_path).map_err(|e| format!("read wat: {e}"))?;
    let _ = fs::remove_file(&wat_path);
    let _ = fs::remove_file(wat_path.with_extension("wasm"));
    let _ = fs::remove_file(wat_path.with_extension("abi.json"));

    let wasm = wat::parse_str(&wat).map_err(|e| format!("assemble: {e}"))?;
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).map_err(|e| format!("module: {e}"))?;
    let mut store = Store::new(&engine, ());
    let mut linker = Linker::new(&engine);
    let env = TestEnv::new();

    let e = env.clone();
    linker
        .func_wrap("env", "print_int", move |v: i32| e.print(&v.to_string()))
        .unwrap();
    let e = env.clone();
    linker
        .func_wrap("env", "print_float", move |v: f32| e.print(&v.to_string()))
        .unwrap();
    let e = env.clone();
    linker
        .func_wrap("env", "print_double", move |v: f64| e.print(&v.to_string()))
        .unwrap();
    let e = env.clone();
    linker
        .func_wrap("env", "print_char", move |v: i32| {
            if let Some(c) = char::from_u32(v as u32) {
                e.print(&c.to_string());
            }
        })
        .unwrap();
    let e = env.clone();
    linker
        .func_wrap(
            "env",
            "print_string",
            move |mut caller: Caller<'_, ()>, ptr: i32| {
                let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                let s = read_string_from_memory(&memory, &caller, ptr);
                e.print(&s);
            },
        )
        .unwrap();

    link_math_functions(&mut linker).unwrap();
    link_file_functions(&mut linker).unwrap();
    link_http_functions(&mut linker).unwrap();
    link_regex_functions(&mut linker).unwrap();

    linker
        .define_unknown_imports_as_traps(&module)
        .map_err(|e| format!("stub imports: {e}"))?;
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| format!("instantiate: {e}"))?;
    let main = instance
        .get_typed_func::<(), ()>(&mut store, "main")
        .map_err(|e| format!("no main: {e}"))?;
    main.call(&mut store, ())
        .map_err(|e| format!("execute: {e}"))?;

    let out = env.output.lock().unwrap().clone();
    Ok(out)
}

#[test]
fn mir_backend_e2e_coverage() {
    let cases_dir = Path::new("tests/cases");
    if !cases_dir.exists() {
        return;
    }
    let xfail: BTreeSet<&str> = XFAIL.iter().map(|(name, _)| *name).collect();

    let mut passed: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut unexpected_pass: Vec<String> = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(cases_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("dream"))
        .collect();
    entries.sort();

    for path in entries {
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        // Cases that are supposed to fail compilation are not backend-coverage cases.
        if path.with_extension("expected_error").exists() {
            continue;
        }
        let expected = match fs::read_to_string(path.with_extension("expected")) {
            Ok(s) => s,
            Err(_) => continue, // no golden output to compare against
        };

        let is_xfail = xfail.contains(stem.as_str());
        match compile_and_run_mir(&path) {
            Ok(actual) if actual.trim() == expected.trim() => {
                if is_xfail {
                    unexpected_pass.push(stem);
                } else {
                    passed.push(stem);
                }
            }
            Ok(actual) => {
                if !is_xfail {
                    failed.push((stem, format!("output mismatch: got {:?}", actual.trim())));
                }
            }
            Err(e) => {
                if !is_xfail {
                    failed.push((stem, e));
                }
            }
        }
    }

    eprintln!(
        "\nMIR backend e2e coverage: {} passing, {} xfail, {} unexpectedly failing",
        passed.len(),
        xfail.len(),
        failed.len()
    );
    eprintln!("passing: {passed:?}");

    if !unexpected_pass.is_empty() {
        eprintln!(
            "\nThese XFAIL cases now PASS — remove them from XFAIL:\n  {unexpected_pass:?}"
        );
    }
    if !failed.is_empty() {
        let detail: String = failed
            .iter()
            .map(|(n, e)| format!("  {n}: {e}"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{} case(s) not in XFAIL failed through the MIR backend:\n{detail}",
            failed.len()
        );
    }
    assert!(
        unexpected_pass.is_empty(),
        "XFAIL is stale (see message above)"
    );
}
