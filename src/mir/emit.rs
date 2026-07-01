//! MIR -> WAT (text WebAssembly) backend.
//!
//! The relooper ([`super::relooper`]) recovers structured shapes from the CFG; this emitter lowers a
//! function to WAT. Control flow uses a labeled-block dispatch loop (a `br_table` over a block-index
//! local), which is correct for any reducible CFG; the relooper shapes are the basis for emitting
//! idiomatic structured `block`/`loop`/`if` instead, the planned refinement. Straight-line
//! statements, operands, and arithmetic are emitted directly. Memory-backed places (struct fields,
//! array elements) and allocation reuse the existing runtime/object/string layers when wired in;
//! they are marked `;; TODO(layout)` here pending that integration.

use super::{
    BinOp, Const, MirFunction, Operand, Place, Rvalue, Statement, Terminator, UnOp,
};
use crate::hir::{scalar_size, LayoutTable};
use crate::types::{DefId, PrimTy, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::fmt::Write;

/// Runtime type tag for arrays passed to `$malloc`, matching the object protocol's `$object_tag`
/// dispatch (mirrors `codegen::wasm::object::TAG_ARRAY`).
const ARRAY_TAG: i32 = crate::codegen::wasm::object::TAG_ARRAY;

/// The first tag assigned to a user struct/union; consecutive types get consecutive tags. Mirrors
/// the legacy backend so the shared runtime's dispatch tables agree (`TAG_STRUCT_BASE`).
const STRUCT_TAG_BASE: i32 = crate::codegen::wasm::object::TAG_STRUCT_BASE;

/// The heap-block tag for strings (mirrors `codegen::wasm::object::TAG_STRING`), written into the
/// header of interned string blocks so the runtime treats them as strings.
const STRING_TAG: i32 = 5;

/// Byte size of the universal heap-block header `[size:i32][tag:i32][ref_count:i32]` that precedes
/// every allocated value; a value's pointer points at `block_start + HEAP_HEADER_SIZE`.
const HEAP_HEADER_SIZE: u32 = 12;

/// Base address (block start) of the interned string data segment. Each string is a heap-object
/// block `[size=0][tag=STRING][ref_count=1][utf8][\0]`; the mapped address points at the utf8 bytes
/// (block start + header), matching the runtime's null-terminated string ABI. The heap starts above.
const STRING_BASE: u32 = 1024;

/// Linear-memory size, in 64 KiB WASM pages.
const MEMORY_PAGES: u32 = 16;

/// The fixed allocator runtime (`$malloc`/`$free`/`$retain`/`$release_generic`/`$object_tag`),
/// shared with the legacy backend as the single source of truth for the heap ABI. Its debug-counter
/// placeholders are expanded to nothing (instrumentation off) here.
const RUNTIME_ALLOCATOR: &str = include_str!("../codegen/wasm/runtime/allocator.wat");

/// The fixed string runtime (`$strlen`/`$char_at`/`$string_eq`/`$concat_strings`/`$string_alloc`/…),
/// shared with the legacy backend. Self-contained given the allocator + memory.
const RUNTIME_STRINGS: &str = include_str!("../codegen/wasm/runtime/strings.wat");

/// The object runtime: box/unbox/hash plus the integer-family `*_to_string` formatters
/// (`$int_to_string`/`$long_to_string`/`$byte_to_string`/…). `{TAG_*}` placeholders are substituted.
const RUNTIME_OBJECT: &str = include_str!("../codegen/wasm/runtime/object.wat");

/// The decimal `float`/`double` formatter (`$float_to_string`/`$double_to_string`). `{minus}` (the
/// data pointer of the interned `"-"`) and `{TAG_STRING}` are substituted.
const RUNTIME_FORMAT: &str = include_str!("../codegen/wasm/runtime/format.wat");

/// String constants the `*_to_string` runtime references by address (`bool` renders to `"true"`/
/// `"false"`; the `double` formatter prepends `"-"`). Interned into every module so the runtime is
/// always self-contained.
const RUNTIME_STR_CONSTS: [&str; 3] = ["true", "false", "-"];

fn runtime_prelude() -> String {
    let mut out = RUNTIME_ALLOCATOR
        .replace(";;@DEBUG_ALLOC_COUNT@", "")
        .replace(";;@DEBUG_FREE_COUNT@", "");
    out.push('\n');
    out.push_str(RUNTIME_STRINGS);
    out
}

/// Builds the `*_to_string` runtime (object formatters + generated `$bool_to_string` + the float/
/// double formatter), resolving the `{TAG_*}`/`{minus}` placeholders and the `bool` string pointers
/// from the interned string table. Depends on the allocator + string runtime emitted before it.
fn to_string_runtime(strings: &IndexMap<String, u32>) -> String {
    use crate::codegen::wasm::object as tags;
    let object = RUNTIME_OBJECT
        .replace("{TAG_INT}", &tags::TAG_INT.to_string())
        .replace("{TAG_FLOAT}", &tags::TAG_FLOAT.to_string())
        .replace("{TAG_DOUBLE}", &tags::TAG_DOUBLE.to_string())
        .replace("{TAG_BOOL}", &tags::TAG_BOOL.to_string())
        .replace("{TAG_STRING}", &tags::TAG_STRING.to_string())
        .replace("{TAG_CHAR}", &tags::TAG_CHAR.to_string())
        .replace("{TAG_LONG}", &tags::TAG_LONG.to_string())
        .replace("{TAG_UINT}", &tags::TAG_UINT.to_string())
        .replace("{TAG_ULONG}", &tags::TAG_ULONG.to_string())
        .replace("{TAG_BYTE}", &tags::TAG_BYTE.to_string());
    let t = strings["true"];
    let f = strings["false"];
    let minus = strings["-"];
    let bool_to_string = format!(
        "(func $bool_to_string (param $v i32) (result i32)\n  local.get $v\n  (if (result i32)\n    (then i32.const {t})\n    (else i32.const {f})))\n"
    );
    let format = RUNTIME_FORMAT
        .replace("{minus}", &minus.to_string())
        .replace("{TAG_STRING}", &tags::TAG_STRING.to_string());
    format!("{object}\n{bool_to_string}\n{format}\n")
}

/// The heap starts (8-byte aligned) above the interned string segment, never below the string base.
/// Each interned string's mapped address points at its data bytes; its block extends `len + 1` bytes
/// beyond that (the utf8 + NUL terminator).
fn heap_base(strings: &IndexMap<String, u32>) -> u32 {
    let end = strings
        .iter()
        .map(|(s, addr)| addr + s.len() as u32 + 1)
        .max()
        .unwrap_or(STRING_BASE);
    (end.max(STRING_BASE) + 7) & !7
}

/// The emitted symbol for a function (or generic instance): the source name, suffixed with the
/// instance's interned type-arg ids so each monomorphization stays distinct.
pub(crate) fn func_symbol(func: &MirFunction) -> String {
    if func.instance.is_empty() {
        func.name.clone()
    } else {
        let args: Vec<String> = func.instance.iter().map(|t| t.0.to_string()).collect();
        format!("{}__{}", func.name, args.join("_"))
    }
}

/// Maps each function's `(DefId, instance args)` to its emitted symbol, so call sites (which carry
/// the callee's def + monomorphization args) resolve to the same symbol the header uses. Keying by
/// the instance args — not the def alone — keeps distinct generic instances distinct.
fn symbol_table(mir: &super::Mir) -> HashMap<(DefId, Vec<TypeId>), String> {
    let mut table: HashMap<(DefId, Vec<TypeId>), String> = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), func_symbol(f)))
        .collect();
    // Imports have no MIR body but are call targets: map their def to the imported `$name` so calls
    // resolve to the import instead of the `$def{N}` fallback.
    for imp in &mir.imports {
        table.insert((imp.def, vec![]), imp.name.clone());
    }
    // Intrinsic externs have no body/import: map their def to the intrinsic key so a call resolves to
    // the runtime helper `$<key>` (e.g. `$string_alloc`) or is recognized as an async intrinsic
    // (`sleep`) rather than falling back to `$def{N}`.
    for (def, key) in &mir.intrinsics {
        table.entry((*def, vec![])).or_insert_with(|| key.clone());
    }
    table
}

/// Maps each function's `(DefId, instance args)` to its declared parameter types, so call sites can
/// apply implicit numeric widening (e.g. an `int`/`float` argument passed to a `double` parameter)
/// to match the callee's WASM signature. Keyed like [`symbol_table`].
fn signature_table(mir: &super::Mir) -> HashMap<(DefId, Vec<TypeId>), Vec<TypeId>> {
    mir.functions
        .iter()
        .map(|f| {
            let params = f.params.iter().map(|p| f.local_ty(*p)).collect();
            ((f.def, f.instance.clone()), params)
        })
        .collect()
}

/// Maps each function's `(DefId, instance args)` to its slot in the module's function table, in
/// `mir.functions` order (so the slot index matches the `(elem ...)` position below). A `FuncRef`
/// resolves to this index; `call_indirect` uses it as the table entry.
fn func_table(mir: &super::Mir) -> HashMap<(DefId, Vec<TypeId>), usize> {
    mir.functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.def, f.instance.clone()), i))
        .collect()
}

/// The canonical `call_indirect` type name + `(param …)`/`(result …)` WASM types for a function-typed
/// `ty` (nullable stripped). Named by its *WASM* signature (so `fun(int)` and `fun(bool)` share one),
/// which is all `call_indirect` distinguishes. `None` if `ty` is not a function type.
fn func_sig(interner: &TypeInterner, ty: TypeId) -> Option<(String, Vec<&'static str>, Option<&'static str>)> {
    match interner.kind(interner.strip_nullable(ty)) {
        TyKind::Func(params, ret) => {
            let ptys: Vec<&'static str> = params.iter().map(|p| wasm_ty_of(interner, *p)).collect();
            let rty = match interner.kind(*ret) {
                TyKind::Void => None,
                _ => Some(wasm_ty_of(interner, *ret)),
            };
            let name = format!("$sig_{}__{}", ptys.join("_"), rty.unwrap_or("v"));
            Some((name, ptys, rty))
        }
        _ => None,
    }
}

/// Emits a `(type …)` declaration for every distinct function signature in the program (one per WASM
/// shape), so `call_indirect` can name its expected type. Over-approximates from all interned function
/// types — spare declarations are harmless.
fn emit_func_signatures(out: &mut String, interner: &TypeInterner) {
    let mut seen: IndexMap<String, (Vec<&'static str>, Option<&'static str>)> = IndexMap::new();
    for (id, kind) in interner.iter_kinds() {
        if matches!(kind, TyKind::Func(..)) {
            if let Some((name, ptys, rty)) = func_sig(interner, id) {
                seen.entry(name).or_insert((ptys, rty));
            }
        }
    }
    for (name, (ptys, rty)) in &seen {
        let params: String = ptys.iter().map(|t| format!(" (param {})", t)).collect();
        let result = rty.map(|t| format!(" (result {})", t)).unwrap_or_default();
        let _ = writeln!(out, "(type {} (func{}{}))", name, params, result);
    }
}

pub(crate) fn poll_symbol(func: &MirFunction) -> String {
    format!("poll_{}", func_symbol(func))
}

pub(crate) fn release_call_for_ty(
    interner: &TypeInterner,
    layouts: &LayoutTable,
    ty: TypeId,
) -> String {
    release_call(interner, layouts, ty)
}

/// Emits the function table and its element section (constructors/sync functions first, then async
/// poll functions), plus the `__indirect_function_table` export.
fn emit_func_table(out: &mut String, mir: &super::Mir) {
    let poll_count = mir.functions.iter().filter(|f| f.is_async).count();
    let n = mir.functions.len() + poll_count;
    if n == 0 {
        return;
    }
    let _ = writeln!(out, "(table $__ft {} funcref)", n);
    let mut syms: Vec<String> = mir.functions.iter().map(|f| format!("${}", func_symbol(f))).collect();
    for f in mir.functions.iter().filter(|f| f.is_async) {
        syms.push(format!("${}", poll_symbol(f)));
    }
    let _ = writeln!(out, "(elem (i32.const 0) {})", syms.join(" "));
    out.push_str("(export \"__indirect_function_table\" (table $__ft))\n");
}

/// Assigns each struct and (discriminated) union a distinct runtime tag, starting at
/// [`STRUCT_TAG_BASE`], in layout-table order (deterministic). The same map drives both the tag
/// stamped at allocation (`New`/`UnionNew`) and the `$object_to_string`/`$print_object` dispatch, so
/// they always agree; the exact numeric value only needs to be self-consistent within a module.
fn struct_tags(mir: &super::Mir) -> HashMap<TypeId, i32> {
    mir.layouts
        .structs
        .keys()
        .chain(mir.layouts.unions.keys())
        .enumerate()
        .map(|(i, ty)| (*ty, STRUCT_TAG_BASE + i as i32))
        .collect()
}

/// The fixed runtime strings the object protocol references: the `null`/`<object>` fallbacks plus
/// each struct's default `to_string` pieces (`"Point { "`, `"x: "`, `", y: "`, `" }"`). Interned
/// alongside the program's own literals so `$<Type>_to_string` can reference their data pointers.
fn protocol_strings(mir: &super::Mir) -> Vec<String> {
    let mut v =
        vec!["null".to_string(), "<object>".to_string(), "[".to_string(), "]".to_string(), ", ".to_string()];
    for layout in mir.layouts.structs.values() {
        v.push(format!("{} {{ ", layout.name));
        for (i, f) in layout.fields.iter().enumerate() {
            v.push(if i == 0 { format!("{}: ", f.name) } else { format!(", {}: ", f.name) });
        }
        v.push(" }".to_string());
    }
    for layout in mir.layouts.unions.values() {
        for variant in &layout.variants {
            let (prefix, labels, suffix) = union_variant_pieces(variant);
            v.push(prefix);
            v.extend(labels);
            v.push(suffix);
        }
    }
    v
}

/// The `(prefix, field-labels, suffix)` literal pieces of a union variant's `to_string`. Data
/// variants render as `Variant(a: <a>, b: <b>)`; unit variants render as just `Variant`.
fn union_variant_pieces(v: &crate::hir::UnionVariant) -> (String, Vec<String>, String) {
    if v.fields.is_empty() {
        return (v.name.clone(), Vec::new(), String::new());
    }
    let prefix = format!("{}(", v.name);
    let labels = v
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| if i == 0 { format!("{}: ", f.name) } else { format!(", {}: ", f.name) })
        .collect();
    (prefix, labels, ")".to_string())
}

/// Interns every string constant in the program to a data pointer, in first-appearance order
/// (deterministic). Each string is a heap-object block `[size=0][tag=STRING][ref_count=1][utf8][\0]`;
/// the mapped address points at the utf8 bytes (block start + [`HEAP_HEADER_SIZE`]), so it is a valid
/// runtime string pointer. Blocks are laid out consecutively, 4-byte aligned.
fn string_table(mir: &super::Mir) -> IndexMap<String, u32> {
    let mut found = Vec::new();
    for f in &mir.functions {
        for b in &f.blocks {
            for s in &b.stmts {
                strings_in_stmt(s, &mut found);
            }
            strings_in_terminator(&b.terminator, &mut found);
        }
    }
    let mut map: IndexMap<String, u32> = IndexMap::new();
    let mut block = STRING_BASE;
    // Seed the constants the `*_to_string`/object-protocol runtime references so they always have
    // stable addresses, regardless of which literals the program itself uses.
    let found = RUNTIME_STR_CONSTS
        .iter()
        .map(|s| s.to_string())
        .chain(protocol_strings(mir))
        .chain(found);
    for s in found {
        if !map.contains_key(&s) {
            let total = HEAP_HEADER_SIZE + s.len() as u32 + 1;
            map.insert(s, block + HEAP_HEADER_SIZE);
            block += (total + 3) & !3;
        }
    }
    map
}

fn strings_in_operand(op: &Operand, out: &mut Vec<String>) {
    match op {
        Operand::Const(Const::Str(s)) => out.push(s.clone()),
        Operand::Copy(Place::Index { index, .. }) => strings_in_operand(index, out),
        _ => {}
    }
}

fn strings_in_rvalue(rv: &Rvalue, out: &mut Vec<String>) {
    match rv {
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::Cast(o, _) => strings_in_operand(o, out),
        Rvalue::Binary(_, a, b) => {
            strings_in_operand(a, out);
            strings_in_operand(b, out);
        }
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. } => {
            args.iter().for_each(|a| strings_in_operand(a, out))
        }
        Rvalue::IndirectCall { target, args } => {
            strings_in_operand(target, out);
            args.iter().for_each(|a| strings_in_operand(a, out));
        }
        Rvalue::FuncRef(_) => {}
    }
}

fn strings_in_stmt(s: &Statement, out: &mut Vec<String>) {
    match s {
        Statement::Assign(place, rv) => {
            if let Place::Index { index, .. } = place {
                strings_in_operand(index, out);
            }
            strings_in_rvalue(rv, out);
        }
        Statement::Retain(o) | Statement::Release(o) => strings_in_operand(o, out),
        Statement::Call { args, .. } => args.iter().for_each(|a| strings_in_operand(a, out)),
        Statement::Print { arg, .. } => strings_in_operand(arg, out),
        Statement::Nop => {}
    }
}

fn strings_in_terminator(t: &Terminator, out: &mut Vec<String>) {
    match t {
        Terminator::If { cond, .. } => strings_in_operand(cond, out),
        Terminator::Switch { value, .. } => strings_in_operand(value, out),
        Terminator::Return(Some(o)) => strings_in_operand(o, out),
        Terminator::AsyncComplete(Some(o)) => strings_in_operand(o, out),
        _ => {}
    }
}

/// Escapes an interned string's full heap-block bytes as `\HH` pairs: the 12-byte header
/// (`size=0`, `tag=STRING`, `ref_count=1`, little-endian i32s), the utf8 bytes, then a NUL
/// terminator. Written at the block start (the mapped address minus [`HEAP_HEADER_SIZE`]).
fn escape_data(s: &str) -> String {
    let mut out = String::new();
    for word in [0_i32, STRING_TAG, 1] {
        for b in word.to_le_bytes() {
            let _ = write!(out, "\\{:02x}", b);
        }
    }
    for b in s.bytes() {
        let _ = write!(out, "\\{:02x}", b);
    }
    out.push_str("\\00");
    out
}

/// Emits a whole MIR program as a sequence of WAT function definitions (no module wrapper). Used by
/// the pipeline tests; the driver target is [`emit_module`].
pub fn emit_program(mir: &super::Mir, interner: &TypeInterner) -> String {
    let symbols = symbol_table(mir);
    let sigs = signature_table(mir);
    let strings = string_table(mir);
    let tags = struct_tags(mir);
    let ftable = func_table(mir);
    let mut out = String::new();
    for f in &mir.functions {
        out.push_str(&emit_function_with(
            f, interner, &symbols, &sigs, &mir.layouts, &strings, &tags, &ftable,
        ));
        out.push('\n');
    }
    out
}

/// Emits a whole MIR program as a single `(module ...)`, exporting every (non-instance) function
/// under its source name. This is the self-contained unit the driver will hand to the WASM
/// assembler once the runtime layers are wired in.
pub fn emit_module(mir: &super::Mir, interner: &TypeInterner) -> String {
    let symbols = symbol_table(mir);
    let sigs = signature_table(mir);
    let strings = string_table(mir);
    let tags = struct_tags(mir);
    let ftable = func_table(mir);
    let mut out = String::new();
    out.push_str("(module\n");

    // Imports come first (WASM requires imported funcs before defined ones).
    emit_imports(&mut out, mir, interner);

    // `call_indirect` signature types (declared before use), plus the function table + its export.
    emit_func_signatures(&mut out, interner);
    emit_func_table(&mut out, mir);

    // Linear memory + allocator runtime state. The heap bump pointer starts above the string data.
    let _ = writeln!(out, "(memory {})", MEMORY_PAGES);
    let _ = writeln!(out, "(global $heap_ptr (mut i32) (i32.const {}))", heap_base(&strings));
    out.push_str("(global $free_list_head (mut i32) (i32.const 0))\n");
    out.push_str("(global $live_objects (mut i32) (i32.const 0))\n");
    out.push_str("(global $total_allocations (mut i32) (i32.const 0))\n");

    // Module-level user variables. They start zeroed; any initializer runs in `$__dream_init`
    // (emitted as a normal function below and wired to `(start ...)`).
    for g in &mir.globals {
        let zero = zero_literal(wasm_ty_of(interner, g.ty));
        let _ = writeln!(out, "(global $g{} (mut {}) {})", g.id.0, wasm_ty_of(interner, g.ty), zero);
    }

    out.push_str(&runtime_prelude());
    out.push('\n');
    if super::async_emit::module_has_async(&mir.functions) {
        out.push_str(&super::async_emit::async_runtime_wat());
        out.push('\n');
    }
    out.push_str(&to_string_runtime(&strings));
    out.push('\n');
    emit_object_protocol(&mut out, mir, interner, &strings, &tags);
    out.push('\n');
    emit_release_funcs(&mut out, mir, interner, &tags);
    out.push('\n');

    for (s, addr) in &strings {
        // The data segment is the full heap block, written at the block start (header before data).
        let block = addr - HEAP_HEADER_SIZE;
        let _ = writeln!(out, "(data (i32.const {}) \"{}\")", block, escape_data(s));
    }

    let polls = super::async_emit::poll_indices(&mir.functions);
    let mut has_init = false;
    for f in &mir.functions {
        if f.is_async {
            out.push_str(&super::async_emit::emit_async_function(
                f, interner, &symbols, &mir.layouts, &strings, &tags, &ftable,
                *polls.get(&(f.def, f.instance.clone())).unwrap_or(&0),
            ));
        } else {
            out.push_str(&emit_function_with(
                f, interner, &symbols, &sigs, &mir.layouts, &strings, &tags, &ftable,
            ));
        }
        if f.name == super::lower::INIT_FN_NAME {
            has_init = true;
        } else if f.instance.is_empty() && f.name == "main" && f.is_async {
            out.push_str(&super::async_emit::emit_async_main_wrapper(
                &func_symbol(f),
                !f.params.is_empty(),
            ));
        } else if f.instance.is_empty() && !(f.name == "main" && f.is_async) {
            let _ = writeln!(out, "(export \"{}\" (func ${}))", f.name, func_symbol(f));
        }
        out.push('\n');
    }

    // Run global initializers before any entry point.
    if has_init {
        let _ = writeln!(out, "(start ${})", super::lower::INIT_FN_NAME);
    }

    // Host-facing exports: memory and the allocator (so a JS runtime can build heap values).
    out.push_str("(export \"memory\" (memory 0))\n");
    out.push_str("(export \"malloc\" (func $malloc))\n");
    out.push_str("(export \"free\" (func $free))\n");
    if super::async_emit::module_has_async(&mir.functions) {
        out.push_str("(export \"__dream_run_loop\" (func $dream_run_loop))\n");
        out.push_str("(export \"__dream_resolve\" (func $dream_resolve))\n");
        out.push_str("(export \"__dream_new_future\" (func $dream_new_future))\n");
    }
    out.push_str(")\n");
    out
}

/// Emits the module's `(import ...)` declarations: the fixed host `print_*` builtins (which
/// `print`/`println` lower to) followed by user `extern fun` interop imports. Call sites reference
/// each import's internal `$name`; the `module`/`field` pair names the host binding.
fn emit_imports(out: &mut String, mir: &super::Mir, interner: &TypeInterner) {
    for (name, param) in [
        ("print_string", "i32"),
        ("print_int", "i32"),
        ("print_float", "f32"),
        ("print_double", "f64"),
        ("print_char", "i32"),
    ] {
        let _ = writeln!(out, "(import \"env\" \"{name}\" (func ${name} (param {param})))");
    }
    for imp in &mir.imports {
        let params: String = imp
            .params
            .iter()
            .map(|t| format!(" {}", wasm_ty_of(interner, *t)))
            .collect();
        let params = if params.is_empty() {
            String::new()
        } else {
            format!(" (param{params})")
        };
        let result = match imp.ret {
            Some(t) => format!(" (result {})", wasm_ty_of(interner, t)),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "(import \"{}\" \"{}\" (func ${}{}{}))",
            imp.module, imp.field, imp.name, params, result
        );
    }
}

/// The WASM value type for a Dream type (`i32`/`i64`/`f32`/`f64`), used for global declarations.
pub(crate) fn wasm_ty_of(interner: &TypeInterner, ty: TypeId) -> &'static str {
    match interner.kind(interner.strip_nullable(ty)) {
        TyKind::Prim(PrimTy::Double) => "f64",
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "i64",
        TyKind::Prim(PrimTy::Float) => "f32",
        _ => "i32",
    }
}

fn zero_literal(wasm_ty: &str) -> &'static str {
    match wasm_ty {
        "f64" => "(f64.const 0)",
        "f32" => "(f32.const 0)",
        "i64" => "(i64.const 0)",
        _ => "(i32.const 0)",
    }
}

/// The load instruction for a value of `ty` (width-aware; sub-word scalars zero-extend). Free
/// counterpart of [`Emitter::load_instr`], used by the generated object-protocol helpers.
fn load_instr_for(interner: &TypeInterner, ty: TypeId) -> &'static str {
    match interner.kind(interner.strip_nullable(ty)) {
        TyKind::Prim(PrimTy::Float) => "f32.load",
        TyKind::Prim(PrimTy::Double) => "f64.load",
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "i64.load",
        TyKind::Prim(PrimTy::Bool | PrimTy::Char | PrimTy::Byte) => "i32.load8_u",
        _ => "i32.load",
    }
}

/// The `$*_to_string` call that turns a loaded value of `ty` into a string pointer, or `None` when
/// the value already *is* a string pointer (`string`, needing no conversion). Enums render as their
/// `int` value; arrays dispatch to their element-typed `$array_to_string_t<id>` (arrays are not
/// self-describing at runtime, so the call is chosen statically); other reference types route through
/// the tag-dispatching `$object_to_string`.
fn value_to_string_call(interner: &TypeInterner, ty: TypeId) -> Option<String> {
    let call = match interner.kind(interner.strip_nullable(ty)) {
        TyKind::Prim(PrimTy::Int) => "$int_to_string",
        TyKind::Prim(PrimTy::Bool) => "$bool_to_string",
        TyKind::Prim(PrimTy::Char) => "$char_to_string",
        TyKind::Prim(PrimTy::Float) => "$float_to_string",
        TyKind::Prim(PrimTy::Double) => "$double_to_string",
        TyKind::Prim(PrimTy::Long) => "$long_to_string",
        TyKind::Prim(PrimTy::ULong) => "$ulong_to_string",
        TyKind::Prim(PrimTy::UInt) => "$uint_to_string",
        TyKind::Prim(PrimTy::Byte) => "$byte_to_string",
        TyKind::Prim(PrimTy::String) => return None,
        TyKind::Enum(_) => "$int_to_string",
        TyKind::Array(elem) => return Some(array_to_string_sym(*elem)),
        _ => "$object_to_string",
    };
    Some(call.to_string())
}

/// The symbol of the generated element-typed array `to_string` helper for element type `elem`.
fn array_to_string_sym(elem: TypeId) -> String {
    format!("$array_to_string_t{}", elem.0)
}

/// Emits the object-protocol runtime that depends on the user's types: one default `$<Type>_to_string`
/// per struct, plus the tag-dispatching `$object_to_string` and `$print_object` routers. Struct
/// `to_string` renders as `Type { field: value, ... }`, recursing into reference fields via
/// `$object_to_string`.
fn emit_object_protocol(
    out: &mut String,
    mir: &super::Mir,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
) {
    for layout in mir.layouts.structs.values() {
        emit_struct_to_string(out, layout, interner, strings);
    }
    for layout in mir.layouts.unions.values() {
        emit_union_to_string(out, layout, interner, strings);
    }
    for elem in array_elem_types(mir, interner) {
        emit_array_to_string(out, elem, interner, strings);
    }
    emit_object_to_string(out, mir, strings, tags);
    // `$print_object`: render via the tag dispatcher, then print the resulting string.
    out.push_str(
        "(func $print_object (param $ptr i32)\n  (local.get $ptr) (call $object_to_string) (call $print_string))\n",
    );
}

/// Emits one struct's default `$<Type>_to_string`, concatenating the interned label pieces with each
/// field's rendered value (in offset order).
fn emit_struct_to_string(
    out: &mut String,
    layout: &crate::hir::TypeLayout,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let prefix = format!("{} {{ ", layout.name);
    let _ = writeln!(out, "(func ${}_to_string (param $this i32) (result i32)", layout.name);
    out.push_str("  (local $res i32)\n");
    let _ = writeln!(out, "  (i32.const {}) (local.set $res)", strings[&prefix]);
    for (i, f) in layout.fields.iter().enumerate() {
        let label = if i == 0 { format!("{}: ", f.name) } else { format!(", {}: ", f.name) };
        let _ = writeln!(
            out,
            "  (local.get $res) (i32.const {}) (call $concat_strings) (local.set $res)",
            strings[&label]
        );
        out.push_str("  (local.get $res)\n  (local.get $this)\n");
        if f.offset > 0 {
            let _ = writeln!(out, "  (i32.const {}) (i32.add)", f.offset);
        }
        let _ = writeln!(out, "  ({})", load_instr_for(interner, f.ty));
        if let Some(call) = value_to_string_call(interner, f.ty) {
            let _ = writeln!(out, "  (call {})", call);
        }
        out.push_str("  (call $concat_strings) (local.set $res)\n");
    }
    let _ = writeln!(out, "  (local.get $res) (i32.const {}) (call $concat_strings)", strings[" }"]);
    out.push_str(")\n");
}

/// Emits one union's default `$<Union>_to_string`: reads the discriminant word (offset 0) and, for
/// the matching variant, renders `Variant(field: value, ...)` (unit variants render as just the
/// variant name). An unrecognized discriminant falls back to `"<object>"`.
fn emit_union_to_string(
    out: &mut String,
    layout: &crate::hir::UnionLayout,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let _ = writeln!(out, "(func ${}_to_string (param $this i32) (result i32)", layout.name);
    out.push_str("  (local $res i32)\n  (local $d i32)\n");
    let _ = writeln!(out, "  (i32.const {}) (local.set $res)", strings["<object>"]);
    out.push_str("  (local.get $this) (i32.load) (local.set $d)\n");
    for variant in &layout.variants {
        let (prefix, labels, suffix) = union_variant_pieces(variant);
        let _ = writeln!(
            out,
            "  (local.get $d) (i32.const {}) (i32.eq) (if (then",
            variant.discriminant
        );
        let _ = writeln!(out, "    (i32.const {}) (local.set $res)", strings[&prefix]);
        for (idx, f) in variant.fields.iter().enumerate() {
            let _ = writeln!(
                out,
                "    (local.get $res) (i32.const {}) (call $concat_strings) (local.set $res)",
                strings[&labels[idx]]
            );
            out.push_str("    (local.get $res)\n    (local.get $this)\n");
            if f.offset > 0 {
                let _ = writeln!(out, "    (i32.const {}) (i32.add)", f.offset);
            }
            let _ = writeln!(out, "    ({})", load_instr_for(interner, f.ty));
            if let Some(call) = value_to_string_call(interner, f.ty) {
                let _ = writeln!(out, "    (call {})", call);
            }
            out.push_str("    (call $concat_strings) (local.set $res)\n");
        }
        let _ = writeln!(
            out,
            "    (local.get $res) (i32.const {}) (call $concat_strings) (local.set $res)",
            strings[&suffix]
        );
        out.push_str("  ))\n");
    }
    out.push_str("  (local.get $res)\n)\n");
}

/// The distinct array **element** types that need a generated `$array_to_string_t<id>`: those
/// reachable as an array-typed struct/union field, local, global, or a direct `print` of an array.
/// Element types that are themselves arrays are added transitively (fixpoint), so nested arrays render
/// (and deep-release) their contents.
fn array_elem_types(mir: &super::Mir, interner: &TypeInterner) -> Vec<TypeId> {
    let mut order: Vec<TypeId> = Vec::new();
    for layout in mir.layouts.structs.values() {
        for f in &layout.fields {
            push_array_elem(&mut order, interner, f.ty);
        }
    }
    for layout in mir.layouts.unions.values() {
        for v in &layout.variants {
            for f in &v.fields {
                push_array_elem(&mut order, interner, f.ty);
            }
        }
    }
    for f in &mir.functions {
        // Any array-typed local can be printed *or* deep-released, both of which need its element
        // helper; covering all locals keeps `$release_array_t<E>`/`$array_to_string_t<E>` references
        // resolvable even for arrays that are only released (never printed).
        for l in &f.locals {
            push_array_elem(&mut order, interner, l.ty);
        }
        for b in &f.blocks {
            for s in &b.stmts {
                if let Statement::Print { ty, .. } = s {
                    push_array_elem(&mut order, interner, *ty);
                }
            }
        }
    }
    for g in &mir.globals {
        push_array_elem(&mut order, interner, g.ty);
    }
    // Fixpoint: an element type that is *itself* an array (`int[][]` → element `int[]`) needs its own
    // inner-element helper; `push_array_elem` unwraps one array level, so re-pushing each element adds it.
    let mut i = 0;
    while i < order.len() {
        let cur = order[i];
        push_array_elem(&mut order, interner, cur);
        i += 1;
    }
    order
}

/// If `ty` (after nullable stripping) is an array, records its element type in `order` (dedup,
/// first-seen order).
fn push_array_elem(order: &mut Vec<TypeId>, interner: &TypeInterner, ty: TypeId) {
    if let Some(e) = interner.unwrap_array(interner.strip_nullable(ty)) {
        if !order.contains(&e) {
            order.push(e);
        }
    }
}

/// Emits one array element type's `$array_to_string_t<id>`: renders `[e0, e1, ...]`, converting each
/// element via [`value_to_string_call`]. The array block is `[len: i32][elem0][elem1]...`.
fn emit_array_to_string(
    out: &mut String,
    elem: TypeId,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let (esize, _) = scalar_size(interner, elem);
    let _ = writeln!(out, "(func {} (param $ptr i32) (result i32)", array_to_string_sym(elem));
    out.push_str("  (local $res i32)\n  (local $len i32)\n  (local $i i32)\n");
    let _ = writeln!(out, "  (i32.const {}) (local.set $res)", strings["["]);
    out.push_str("  (local.get $ptr) (i32.load) (local.set $len)\n");
    out.push_str("  (i32.const 0) (local.set $i)\n");
    out.push_str("  (block $done (loop $scan\n");
    out.push_str("    (local.get $i) (local.get $len) (i32.ge_s) (br_if $done)\n");
    let _ = writeln!(
        out,
        "    (local.get $i) (i32.const 0) (i32.gt_s) (if (then (local.get $res) (i32.const {}) (call $concat_strings) (local.set $res)))",
        strings[", "]
    );
    out.push_str("    (local.get $res)\n    (local.get $ptr) (i32.const 4) (i32.add)\n");
    if esize == 1 {
        out.push_str("    (local.get $i) (i32.add)\n");
    } else {
        let _ = writeln!(out, "    (local.get $i) (i32.const {}) (i32.mul) (i32.add)", esize);
    }
    let _ = writeln!(out, "    ({})", load_instr_for(interner, elem));
    if let Some(call) = value_to_string_call(interner, elem) {
        let _ = writeln!(out, "    (call {})", call);
    }
    out.push_str("    (call $concat_strings) (local.set $res)\n");
    out.push_str("    (local.get $i) (i32.const 1) (i32.add) (local.set $i)\n");
    out.push_str("    (br $scan)))\n");
    let _ = writeln!(out, "  (local.get $res) (i32.const {}) (call $concat_strings)", strings["]"]);
    out.push_str(")\n");
}

/// Emits `$object_to_string`: null → `"null"`, boxed primitives → unbox + `*_to_string`, strings →
/// identity, each struct/union tag → its `$<Type>_to_string`, everything else → `"<object>"`.
fn emit_object_to_string(
    out: &mut String,
    mir: &super::Mir,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
) {
    use crate::codegen::wasm::object as t;
    out.push_str("(func $object_to_string (param $ptr i32) (result i32)\n  (local $tag i32)\n");
    let _ = writeln!(
        out,
        "  (local.get $ptr) (i32.eqz) (if (then (i32.const {}) (return)))",
        strings["null"]
    );
    out.push_str("  (local.get $ptr) (call $object_tag) (local.set $tag)\n");
    let prim_arms: [(i32, &str, &str); 9] = [
        (t::TAG_INT, "$unbox_int", "$int_to_string"),
        (t::TAG_FLOAT, "$unbox_float", "$float_to_string"),
        (t::TAG_DOUBLE, "$unbox_double", "$double_to_string"),
        (t::TAG_BOOL, "$unbox_bool", "$bool_to_string"),
        (t::TAG_CHAR, "$unbox_char", "$char_to_string"),
        (t::TAG_LONG, "$unbox_long", "$long_to_string"),
        (t::TAG_ULONG, "$unbox_ulong", "$ulong_to_string"),
        (t::TAG_UINT, "$unbox_uint", "$uint_to_string"),
        (t::TAG_BYTE, "$unbox_byte", "$byte_to_string"),
    ];
    for (tag, unbox, to_str) in prim_arms {
        write_tag_arm(out, tag, &format!("(local.get $ptr) (call {}) (call {})", unbox, to_str));
    }
    // Strings are already their own pointer.
    write_tag_arm(out, t::TAG_STRING, "(local.get $ptr)");
    for (ty, layout) in &mir.layouts.structs {
        if let Some(&tag) = tags.get(ty) {
            write_tag_arm(out, tag, &format!("(local.get $ptr) (call ${}_to_string)", layout.name));
        }
    }
    for (ty, layout) in &mir.layouts.unions {
        if let Some(&tag) = tags.get(ty) {
            write_tag_arm(out, tag, &format!("(local.get $ptr) (call ${}_to_string)", layout.name));
        }
    }
    let _ = writeln!(out, "  (i32.const {})\n)", strings["<object>"]);
}

/// Writes one `if (tag == n) {{ <body>; return }}` dispatch arm into `$object_to_string`.
fn write_tag_arm(out: &mut String, tag: i32, body: &str) {
    let _ = writeln!(
        out,
        "  (local.get $tag) (i32.const {}) (i32.eq) (if (then {} (return)))",
        tag, body
    );
}

/// Maps a callee symbol to an async-intrinsic kind (`sleep`, `__promise_all`, …), if any.
fn async_intrinsic_kind(sym: &str) -> Option<&'static str> {
    use crate::intrinsics;
    if sym.ends_with("_sleep") || sym == intrinsics::SLEEP {
        Some(intrinsics::SLEEP)
    } else if sym == intrinsics::PROMISE_ALL {
        Some(intrinsics::PROMISE_ALL)
    } else if sym == intrinsics::PROMISE_ANY || sym == intrinsics::PROMISE_RACE {
        Some(intrinsics::PROMISE_ANY)
    } else {
        None
    }
}

/// The `$release_*` symbol that deep-releases a reference value of `ty` (chosen *statically* from the
/// declared type): structs/unions call their generated per-type release, reference-element arrays
/// their element-typed array release, and everything else (strings, scalar arrays, boxed primitives)
/// drops one reference via the generic runtime. `object`-typed values route through the tag-dispatched
/// `$release_object` since their concrete type is unknown until runtime. Callers guard on
/// [`TypeInterner::is_reference`] first, so non-reference types never reach here.
fn release_call(interner: &TypeInterner, layouts: &LayoutTable, ty: TypeId) -> String {
    let ty = interner.strip_nullable(ty);
    match interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            if let Some(l) = layouts.structs.get(&ty) {
                format!("$release_{}", l.name)
            } else if let Some(l) = layouts.unions.get(&ty) {
                format!("$release_{}", l.name)
            } else {
                "$release_object".to_string()
            }
        }
        TyKind::Array(e) if interner.is_reference(*e) => format!("$release_array_t{}", e.0),
        TyKind::Object => "$release_object".to_string(),
        _ => "$release_generic".to_string(),
    }
}

/// Emits the null check + refcount decrement shared by every per-type release, opening the
/// `if (new_count == 0) (then` block that the caller fills with the deep-release + `$free`. Uses only
/// the `$rc`/`$nc` locals, which every release function declares. Matches `$release_generic`'s ABI
/// (refcount word at `ptr - 4`).
fn emit_release_prologue(out: &mut String) {
    out.push_str("  (local.get $ptr) (i32.eqz) (if (then (return)))\n");
    out.push_str("  (local.get $ptr) (i32.const 4) (i32.sub) (local.set $rc)\n");
    out.push_str("  (local.get $rc) (i32.load) (i32.const 1) (i32.sub) (local.set $nc)\n");
    out.push_str("  (local.get $rc) (local.get $nc) (i32.store)\n");
    out.push_str("  (local.get $nc) (i32.eqz) (if (then\n");
}

/// Emits the `del()` destructor invocation (when the type declares one): the refcount is first pinned
/// to 1 so the destructor body's own `this` retain/release cannot re-enter this release at zero, then
/// `$<Type>_del(ptr)` runs while the fields are still live. `del` is the destructor's function symbol
/// or `None`.
fn emit_del_call(out: &mut String, del: Option<&str>) {
    if let Some(d) = del {
        out.push_str("    (local.get $rc) (i32.const 1) (i32.store)\n");
        let _ = writeln!(out, "    (local.get $ptr) (call ${})", d);
    }
}

/// Emits the deep-release runtime: a per-struct/union `$release_<Type>` (run `del()` if present,
/// release reference fields, then `$free`), a `$release_array_t<E>` for each reference-element array
/// type, and the tag-dispatching `$release_object`. Non-reference fields and scalar arrays never need
/// releasing; strings/boxed primitives fall through to `$release_generic`.
fn emit_release_funcs(
    out: &mut String,
    mir: &super::Mir,
    interner: &TypeInterner,
    tags: &HashMap<TypeId, i32>,
) {
    let fn_names: std::collections::HashSet<&str> =
        mir.functions.iter().map(|f| f.name.as_str()).collect();
    let del_of = |name: &str| -> Option<String> {
        let sym = format!("{}_del", name);
        fn_names.contains(sym.as_str()).then_some(sym)
    };

    for layout in mir.layouts.structs.values() {
        let del = del_of(&layout.name);
        let _ = writeln!(out, "(func $release_{} (param $ptr i32)", layout.name);
        out.push_str("  (local $rc i32) (local $nc i32)\n");
        emit_release_prologue(out);
        emit_del_call(out, del.as_deref());
        for f in layout.fields.iter().filter(|f| interner.is_reference(f.ty)) {
            out.push_str("    (local.get $ptr)\n");
            if f.offset > 0 {
                let _ = writeln!(out, "    (i32.const {}) (i32.add)", f.offset);
            }
            let _ = writeln!(
                out,
                "    (i32.load) (call {})",
                release_call(interner, &mir.layouts, f.ty)
            );
        }
        out.push_str("    (local.get $ptr) (call $free)\n  ))\n)\n");
    }

    for layout in mir.layouts.unions.values() {
        let del = del_of(&layout.name);
        let _ = writeln!(out, "(func $release_{} (param $ptr i32)", layout.name);
        out.push_str("  (local $rc i32) (local $nc i32) (local $d i32)\n");
        emit_release_prologue(out);
        emit_del_call(out, del.as_deref());
        // Only the active variant's payload is valid, so switch on the discriminant (offset 0).
        out.push_str("    (local.get $ptr) (i32.load) (local.set $d)\n");
        for v in &layout.variants {
            let ref_fields: Vec<&crate::hir::FieldLayout> =
                v.fields.iter().filter(|f| interner.is_reference(f.ty)).collect();
            if ref_fields.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "    (local.get $d) (i32.const {}) (i32.eq) (if (then",
                v.discriminant
            );
            for f in ref_fields {
                out.push_str("      (local.get $ptr)\n");
                if f.offset > 0 {
                    let _ = writeln!(out, "      (i32.const {}) (i32.add)", f.offset);
                }
                let _ = writeln!(
                    out,
                    "      (i32.load) (call {})",
                    release_call(interner, &mir.layouts, f.ty)
                );
            }
            out.push_str("    ))\n");
        }
        out.push_str("    (local.get $ptr) (call $free)\n  ))\n)\n");
    }

    // One array release per reference-element array type; the element type is known statically at the
    // call site, so array releases (unlike `$release_object`) can recurse into their elements.
    for elem in array_elem_types(mir, interner) {
        if !interner.is_reference(elem) {
            continue;
        }
        let _ = writeln!(out, "(func $release_array_t{} (param $ptr i32)", elem.0);
        out.push_str("  (local $rc i32) (local $nc i32) (local $len i32) (local $i i32) (local $elem i32)\n");
        emit_release_prologue(out);
        out.push_str("    (local.get $ptr) (i32.load) (local.set $len)\n");
        out.push_str("    (i32.const 0) (local.set $i)\n");
        out.push_str("    (block $done (loop $scan\n");
        out.push_str("      (local.get $i) (local.get $len) (i32.ge_s) (br_if $done)\n");
        out.push_str("      (local.get $ptr) (i32.const 4) (i32.add) (local.get $i) (i32.const 4) (i32.mul) (i32.add) (i32.load) (local.set $elem)\n");
        let _ = writeln!(
            out,
            "      (local.get $elem) (if (then (local.get $elem) (call {})))",
            release_call(interner, &mir.layouts, elem)
        );
        out.push_str("      (local.get $i) (i32.const 1) (i32.add) (local.set $i) (br $scan)))\n");
        out.push_str("    (local.get $ptr) (call $free)\n  ))\n)\n");
    }

    // `$release_object`: tag dispatch for reference values whose static type is `object`. Strings,
    // boxed primitives, and arrays (not self-describing about their element type) fall through to the
    // shallow generic release.
    out.push_str("(func $release_object (param $ptr i32)\n  (local $tag i32)\n");
    out.push_str("  (local.get $ptr) (i32.eqz) (if (then (return)))\n");
    out.push_str("  (local.get $ptr) (call $object_tag) (local.set $tag)\n");
    for (ty, layout) in &mir.layouts.structs {
        if let Some(&tag) = tags.get(ty) {
            let _ = writeln!(
                out,
                "  (local.get $tag) (i32.const {}) (i32.eq) (if (then (local.get $ptr) (call $release_{}) (return)))",
                tag, layout.name
            );
        }
    }
    for (ty, layout) in &mir.layouts.unions {
        if let Some(&tag) = tags.get(ty) {
            let _ = writeln!(
                out,
                "  (local.get $tag) (i32.const {}) (i32.eq) (if (then (local.get $ptr) (call $release_{}) (return)))",
                tag, layout.name
            );
        }
    }
    out.push_str("  (local.get $ptr) (call $release_generic)\n)\n");
}

/// Emits one function as WAT (calls fall back to `$def{N}`, and field/index access has no layout, so
/// this is for layout-free unit tests; the pipeline uses [`emit_program`]/[`emit_module`]).
pub fn emit_function(func: &MirFunction, interner: &TypeInterner) -> String {
    emit_function_with(
        func,
        interner,
        &HashMap::new(),
        &HashMap::new(),
        &LayoutTable::default(),
        &IndexMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_function_with(
    func: &MirFunction,
    interner: &TypeInterner,
    symbols: &HashMap<(DefId, Vec<TypeId>), String>,
    sigs: &HashMap<(DefId, Vec<TypeId>), Vec<TypeId>>,
    layouts: &LayoutTable,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
    func_table: &HashMap<(DefId, Vec<TypeId>), usize>,
) -> String {
    let mut e = Emitter {
        func,
        interner,
        symbols,
        sigs,
        layouts,
        strings,
        tags,
        func_table,
        out: String::new(),
        async_parent: None,
    };
    e.emit();
    e.out
}

/// Emits one basic block's straight-line body (no CFG dispatch loop). Used by async poll segments.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_straight_line_segment(
    func: &MirFunction,
    interner: &TypeInterner,
    symbols: &HashMap<(DefId, Vec<TypeId>), String>,
    layouts: &LayoutTable,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
    ftable: &HashMap<(DefId, Vec<TypeId>), usize>,
    async_parent: &MirFunction,
) -> String {
    // Async poll segments do not apply call-argument widening yet (async cases are still gated); an
    // empty signature map disables it without extra plumbing through the coroutine transform.
    let sigs: HashMap<(DefId, Vec<TypeId>), Vec<TypeId>> = HashMap::new();
    let mut e = Emitter {
        func,
        interner,
        symbols,
        sigs: &sigs,
        layouts,
        strings,
        tags,
        func_table: ftable,
        out: String::new(),
        async_parent: Some(async_parent),
    };
    let block = func.block(func.entry);
    for stmt in &block.stmts {
        e.emit_stmt(stmt);
    }
    e.emit_poll_terminator(&block.terminator);
    e.out
}

/// Evaluates a HIR expression and stores the result in `$__scratch` (async poll suspend).
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_expr_to_scratch(
    hir: &crate::hir::HFunction,
    expr: &crate::hir::HExpr,
    interner: &TypeInterner,
    symbols: &HashMap<(DefId, Vec<TypeId>), String>,
    layouts: &LayoutTable,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
    ftable: &HashMap<(DefId, Vec<TypeId>), usize>,
    parent: &MirFunction,
) -> String {
    let (mir, temp) = super::lower::lower_expr_value(hir, expr, interner);
    let mut out = emit_straight_line_segment(
        &mir, interner, symbols, layouts, strings, tags, ftable, parent,
    );
    let _ = writeln!(out, "     (local.get ${})", temp.0);
    out.push_str("     (local.set $__scratch)\n");
    out
}

struct Emitter<'a> {
    func: &'a MirFunction,
    interner: &'a TypeInterner,
    symbols: &'a HashMap<(DefId, Vec<TypeId>), String>,
    /// Callee `(def, instance)` → parameter types, for implicit widening of call arguments.
    sigs: &'a HashMap<(DefId, Vec<TypeId>), Vec<TypeId>>,
    layouts: &'a LayoutTable,
    strings: &'a IndexMap<String, u32>,
    tags: &'a HashMap<TypeId, i32>,
    func_table: &'a HashMap<(DefId, Vec<TypeId>), usize>,
    out: String,
    /// When emitting inside an async poll segment, the enclosing task (for scope-exit release).
    async_parent: Option<&'a MirFunction>,
}

impl Emitter<'_> {
    fn line(&mut self, s: &str) {
        let _ = writeln!(self.out, "{}", s);
    }

    /// The symbol for a call target: the resolved function symbol for `(def, instance args)` when
    /// known, else a `def{N}` fallback (runtime intrinsics and not-yet-emitted targets).
    fn callee_symbol(&self, callee: &super::Callee) -> String {
        self.symbols
            .get(&(callee.def, callee.args.clone()))
            .cloned()
            .unwrap_or_else(|| format!("def{}", callee.def.0))
    }

    fn emit(&mut self) {
        let params: String = self
            .func
            .params
            .iter()
            .map(|p| format!(" (param ${} {})", p.0, self.wasm_ty(self.func.local_ty(*p))))
            .collect();
        let result = match self.interner.kind(self.func.ret) {
            TyKind::Void => String::new(),
            _ => format!(" (result {})", self.wasm_ty(self.func.ret)),
        };
        self.line(&format!("(func ${}{}{}", func_symbol(self.func), params, result));

        // Non-parameter locals plus the dispatch program-counter.
        let param_count = self.func.params.len();
        for (i, decl) in self.func.locals.iter().enumerate() {
            if i < param_count {
                continue;
            }
            self.line(&format!("  (local ${} {})", i, self.wasm_ty(decl.ty)));
        }
        self.line("  (local $__pc i32)");
        // Scratch pointer holding the object under construction across field initialization
        // (`New`/`ArrayLit`). Safe as a single slot: lowering materializes all args into operands,
        // so allocations never nest within a single rvalue.
        self.line("  (local $__obj i32)");

        self.emit_dispatch();
        self.line(")");
    }

    /// The labeled-block dispatch loop: each iteration reads `$__pc` and `br_table`s to the matching
    /// block; each block body ends by setting `$__pc` and branching back, or by returning.
    fn emit_dispatch(&mut self) {
        let n = self.func.blocks.len();
        self.line(&format!("  ;; entry = bb{}", self.func.entry.0));
        self.line(&format!("  (i32.const {})", self.func.entry.0));
        self.line("  (local.set $__pc)");
        self.line("  (block $__exit");
        self.line("   (loop $__loop");

        // Open one block per CFG block, innermost = bb0.
        for i in (0..n).rev() {
            self.line(&format!("    (block $bb{}", i));
        }
        // Dispatch from the innermost scope.
        let labels: String = (0..n).map(|i| format!("$bb{} ", i)).collect();
        let default = format!("$bb{}", n.saturating_sub(1));
        self.line(&format!(
            "     (br_table {}{} (local.get $__pc))",
            labels, default
        ));

        // After each `(block $bbK ...)` closes, that block's body runs.
        for i in 0..n {
            self.line(&format!("    ) ;; bb{} body", i));
            self.emit_block(super::BlockId(i as u32));
        }

        self.line("   )"); // loop
        self.line("  )"); // exit block
        // Every block ends in a `return`/`goto`, so control never falls out of the dispatch loop.
        // A value-returning function still needs its implicit `end` to be well-typed; mark the
        // unreachable tail so the validator does not demand a phantom result value on the stack.
        if !matches!(self.interner.kind(self.func.ret), TyKind::Void) {
            self.line("  (unreachable)");
        }
    }

    fn emit_block(&mut self, id: super::BlockId) {
        let block = self.func.block(id);
        for stmt in &block.stmts {
            self.emit_stmt(stmt);
        }
        self.emit_terminator(&block.terminator);
    }

    fn emit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Assign(place, rvalue) => self.emit_assign(place, rvalue),
            Statement::Retain(o) => {
                self.emit_operand(o);
                self.line("     (call $retain)");
            }
            Statement::Release(o) => {
                // Deep release by the operand's declared type: structs/unions/reference arrays run
                // their generated `$release_<...>` (freeing fields + `del()`); other references fall
                // back to the generic/tag-dispatched runtime.
                let ty = self.operand_ty(o);
                let call = if self.interner.is_reference(ty) {
                    release_call(self.interner, self.layouts, ty)
                } else {
                    "$release_generic".to_string()
                };
                self.emit_operand(o);
                self.line(&format!("     (call {})", call));
            }
            Statement::Call { callee, args } => {
                self.emit_call_args(callee, args);
                self.line(&format!("     (call ${})", self.callee_symbol(callee)));
                if !matches!(self.interner.kind(callee.ret), TyKind::Void) {
                    self.line("     (drop)");
                }
            }
            Statement::Print { arg, ty, newline } => {
                // Push the value, then print it. `int`/`char`/`string` go straight to a host import;
                // every other scalar is first rendered with its in-wasm `*_to_string` and printed as a
                // string. `println` appends a trailing newline (`\n` = 10) via `$print_char`.
                self.emit_operand(arg);
                match self.interner.kind(self.interner.strip_nullable(*ty)) {
                    TyKind::Prim(PrimTy::Int) => self.line("     (call $print_int)"),
                    TyKind::Prim(PrimTy::Char) => self.line("     (call $print_char)"),
                    TyKind::Prim(PrimTy::String) => self.line("     (call $print_string)"),
                    TyKind::Prim(prim) => {
                        let to_string = match prim {
                            PrimTy::Bool => "$bool_to_string",
                            PrimTy::Float => "$float_to_string",
                            PrimTy::Double => "$double_to_string",
                            PrimTy::Long => "$long_to_string",
                            PrimTy::UInt => "$uint_to_string",
                            PrimTy::ULong => "$ulong_to_string",
                            PrimTy::Byte => "$byte_to_string",
                            // Int/Char/String handled above; any other primitive prints via $print_int.
                            _ => "",
                        };
                        if to_string.is_empty() {
                            self.line("     (call $print_int)");
                        } else {
                            self.line(&format!("     (call {})", to_string));
                            self.line("     (call $print_string)");
                        }
                    }
                    // Enums are `i32` values at runtime; print their numeric value.
                    TyKind::Enum(_) => self.line("     (call $print_int)"),
                    // Arrays aren't self-describing at runtime (the header only says `TAG_ARRAY`), so
                    // the element-typed `to_string` is chosen statically here, then printed.
                    TyKind::Array(elem) => {
                        self.line(&format!("     (call {})", array_to_string_sym(*elem)));
                        self.line("     (call $print_string)");
                    }
                    // Structs, unions, and `object` render through the tag-dispatching `$print_object`
                    // (which routes to each type's `to_string`).
                    _ => self.line("     (call $print_object)"),
                }
                if *newline {
                    self.line("     (i32.const 10)");
                    self.line("     (call $print_char)");
                }
            }
            Statement::Nop => {}
        }
    }

    fn emit_assign(&mut self, place: &Place, rvalue: &Rvalue) {
        match place {
            Place::Local(l) => {
                self.emit_rvalue(rvalue);
                self.line(&format!("     (local.set ${})", l.0));
            }
            Place::Global(g) => {
                self.emit_rvalue(rvalue);
                self.line(&format!("     (global.set $g{})", g.0));
            }
            Place::Field { base, field } => {
                if let Some((off, fty)) = self.field_layout(*base, *field) {
                    self.field_addr(*base, off);
                    self.emit_rvalue(rvalue);
                    self.line(&format!("     ({})", self.store_instr(fty)));
                    self.retain_stored_rvalue(fty, rvalue);
                } else {
                    self.emit_rvalue(rvalue);
                    self.line("     (drop) ;; TODO(layout): store to field");
                }
            }
            Place::Index { base, index } => {
                if let Some(ety) = self.array_elem_ty(*base) {
                    self.elem_addr(*base, ety, index);
                    self.emit_rvalue(rvalue);
                    self.line(&format!("     ({})", self.store_instr(ety)));
                    self.retain_stored_rvalue(ety, rvalue);
                } else {
                    self.emit_rvalue(rvalue);
                    self.line("     (drop) ;; TODO(layout): store to index");
                }
            }
        }
    }

    /// Pushes the address of `base.field` (`base + offset`) onto the stack.
    /// The runtime tag to stamp into a newly allocated value of `ty`: its assigned struct/union tag,
    /// or the `DefId` as a last-resort fallback (only when no layout/tag is registered).
    fn type_tag(&self, ty: TypeId, fallback: DefId) -> i32 {
        self.tags.get(&ty).copied().unwrap_or(fallback.0 as i32)
    }

    fn field_addr(&mut self, base: super::Local, offset: u32) {
        self.line(&format!("     (local.get ${})", base.0));
        if offset > 0 {
            self.line(&format!("     (i32.const {})", offset));
            self.line("     (i32.add)");
        }
    }

    /// Stores `value` into the object under construction (`$__obj + offset`) with the field/element
    /// width. Used by `New`/`ArrayLit` initialization. A *borrowed* reference (a copy of an existing
    /// place) is retained, since the container becomes a new owner; an owned producer is not
    /// materialized here (lowering routes those through a temporary that is itself released at scope
    /// exit), so retaining a copied operand is the sound, uniform rule.
    fn store_at_obj(&mut self, offset: u32, value_ty: TypeId, value: &Operand) {
        self.line("     (local.get $__obj)");
        if offset > 0 {
            self.line(&format!("     (i32.const {})", offset));
            self.line("     (i32.add)");
        }
        self.emit_operand(value);
        self.line(&format!("     ({})", self.store_instr(value_ty)));
        self.retain_container_value(value_ty, value);
    }

    /// Emits a `$retain` of a reference value being stored into a container (struct field, array
    /// element, or union payload), so the container owns its own reference count. A no-op for
    /// non-reference values and for non-place operands (constants/null; `$retain` also null-guards).
    fn retain_container_value(&mut self, value_ty: TypeId, value: &Operand) {
        let borrowed =
            matches!(value, Operand::Copy(_) | Operand::Const(Const::Str(_)));
        if self.interner.is_reference(value_ty) && borrowed {
            self.emit_operand(value);
            self.line("     (call $retain)");
        }
    }

    /// Like [`Self::retain_container_value`] but for a field/element written from an rvalue: a
    /// *borrowed* value (`Use(Copy(place))`) is retained, while an owned producer (call/new/array
    /// literal result) transfers its `+1` into the container and is left as-is.
    fn retain_stored_rvalue(&mut self, ty: TypeId, rvalue: &Rvalue) {
        if let Rvalue::Use(value) = rvalue {
            self.retain_container_value(ty, value);
        }
    }

    /// Writes a zero of `field_ty`'s width into the object under construction (`$__obj + offset`).
    /// Used to clear a struct before a user constructor runs (reused heap blocks are not zeroed).
    fn zero_at_obj(&mut self, offset: u32, field_ty: TypeId) {
        self.line("     (local.get $__obj)");
        if offset > 0 {
            self.line(&format!("     (i32.const {})", offset));
            self.line("     (i32.add)");
        }
        let zero = match self.store_instr(field_ty) {
            "f64.store" => "(f64.const 0)",
            "f32.store" => "(f32.const 0)",
            "i64.store" => "(i64.const 0)",
            _ => "(i32.const 0)",
        };
        self.line(&format!("     {}", zero));
        self.line(&format!("     ({})", self.store_instr(field_ty)));
    }

    /// Pushes the address of `base[index]` (`base + 4 + index * elem_size`) onto the stack. The
    /// length occupies the first word, so element 0 is at offset 4.
    fn elem_addr(&mut self, base: super::Local, elem_ty: TypeId, index: &Operand) {
        let (size, _) = scalar_size(self.interner, elem_ty);
        self.line(&format!("     (local.get ${})", base.0));
        self.line("     (i32.const 4)");
        self.line("     (i32.add)");
        self.emit_operand(index);
        self.line(&format!("     (i32.const {})", size));
        self.line("     (i32.mul)");
        self.line("     (i32.add)");
    }

    /// The struct field's `(byte offset, type)` from the layout table, or `None` when `base` is not a
    /// laid-out nominal type (e.g. a union, or a type whose layout was not recorded).
    fn field_layout(&self, base: super::Local, field: usize) -> Option<(u32, TypeId)> {
        let bty = self.interner.strip_nullable(self.func.local_ty(base));
        // Layouts are keyed by the full (monomorphized) type id, so `Box<int>` and `Box<string>`
        // resolve to their own field widths.
        let f = self.layouts.get(bty)?.fields.get(field)?;
        Some((f.offset, f.ty))
    }

    /// The element type of an array-typed local, or `None` if `base` is not an array.
    fn array_elem_ty(&self, base: super::Local) -> Option<TypeId> {
        match self.interner.kind(self.interner.strip_nullable(self.func.local_ty(base))) {
            TyKind::Array(e) => Some(*e),
            _ => None,
        }
    }

    /// The load instruction for a value of `ty` (width- and float-aware; sub-word loads are unsigned).
    fn load_instr(&self, ty: TypeId) -> &'static str {
        match self.interner.kind(self.interner.strip_nullable(ty)) {
            TyKind::Prim(PrimTy::Float) => "f32.load",
            TyKind::Prim(PrimTy::Double) => "f64.load",
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "i64.load",
            TyKind::Prim(PrimTy::Bool | PrimTy::Char | PrimTy::Byte) => "i32.load8_u",
            _ => "i32.load",
        }
    }

    /// The store instruction matching [`Self::load_instr`].
    fn store_instr(&self, ty: TypeId) -> &'static str {
        match self.interner.kind(self.interner.strip_nullable(ty)) {
            TyKind::Prim(PrimTy::Float) => "f32.store",
            TyKind::Prim(PrimTy::Double) => "f64.store",
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "i64.store",
            TyKind::Prim(PrimTy::Bool | PrimTy::Char | PrimTy::Byte) => "i32.store8",
            _ => "i32.store",
        }
    }

    fn emit_rvalue(&mut self, rvalue: &Rvalue) {
        match rvalue {
            Rvalue::Use(o) => self.emit_operand(o),
            Rvalue::Binary(op, a, b) => {
                self.emit_operand(a);
                self.emit_operand(b);
                let ty = self.operand_ty(a);
                self.line(&format!("     ({})", self.binop_instr(*op, ty)));
            }
            Rvalue::Unary(op, a) => {
                let ty = self.operand_ty(a);
                match op {
                    UnOp::Neg => {
                        // No `neg` for integers in WASM: 0 - x.
                        if matches!(self.interner.kind(ty), TyKind::Prim(PrimTy::Float | PrimTy::Double)) {
                            self.emit_operand(a);
                            self.line(&format!("     ({}.neg)", self.wasm_ty(ty)));
                        } else {
                            self.line(&format!("     ({}.const 0)", self.wasm_ty(ty)));
                            self.emit_operand(a);
                            self.line(&format!("     ({}.sub)", self.wasm_ty(ty)));
                        }
                    }
                    UnOp::Not => {
                        self.emit_operand(a);
                        self.line("     (i32.eqz)");
                    }
                }
            }
            Rvalue::Call { callee, args } => {
                let sym = self.callee_symbol(callee);
                if let Some(kind) = async_intrinsic_kind(&sym) {
                    self.emit_async_intrinsic(kind, args);
                } else {
                    self.emit_call_args(callee, args);
                    self.line(&format!("     (call ${sym})"));
                }
            }
            Rvalue::IndirectCall { target, args } => {
                for a in args {
                    self.emit_operand(a);
                }
                self.emit_operand(target);
                // The table index (target) is on top of the stack; dispatch through `$__ft` with the
                // signature derived from the target's function type.
                let sig = func_sig(self.interner, self.operand_ty(target))
                    .map(|(name, _, _)| name)
                    .unwrap_or_else(|| "$sig___v".to_string());
                self.line(&format!("     (call_indirect $__ft (type {}))", sig));
            }
            Rvalue::FuncRef(callee) => {
                // A function value is its slot index in the module function table.
                let idx = self
                    .func_table
                    .get(&(callee.def, callee.args.clone()))
                    .copied()
                    .unwrap_or(0);
                self.line(&format!("     (i32.const {}) ;; funcref def{}", idx, callee.def.0));
            }
            Rvalue::New { def, ty, ctor, args } => {
                // `$malloc(data_size, tag)` returns a data pointer with refcount 1.
                let info = self
                    .layouts
                    .get(*ty)
                    .map(|l| (l.size, l.fields.iter().map(|f| (f.offset, f.ty)).collect::<Vec<_>>()));
                if let Some((size, fields)) = info {
                    self.line(&format!("     (i32.const {})", size));
                    self.line(&format!("     (i32.const {}) ;; tag", self.type_tag(*ty, *def)));
                    self.line("     (call $malloc)");
                    self.line("     (local.set $__obj)");
                    if let Some(ctor) = ctor {
                        // A user `constructor(this, args...)` sets the fields itself. Reused heap
                        // blocks are not zeroed, so zero every field first (a constructor that leaves a
                        // field unset must observe 0/null), then call it; the object is the result.
                        for &(off, fty) in &fields {
                            self.zero_at_obj(off, fty);
                        }
                        self.line("     (local.get $__obj)");
                        for arg in args {
                            self.emit_operand(arg);
                        }
                        let sym = self.callee_symbol(&super::Callee {
                            def: *ctor,
                            args: vec![],
                            ret: self.interner.void(),
                        });
                        self.line(&format!("     (call ${})", sym));
                        self.line("     (local.get $__obj)");
                    } else {
                        // Auto constructor: initialize each field from its positional argument.
                        for (i, arg) in args.iter().enumerate() {
                            if let Some(&(off, fty)) = fields.get(i) {
                                self.store_at_obj(off, fty, arg);
                            }
                        }
                        self.line("     (local.get $__obj)");
                    }
                } else {
                    for a in args {
                        self.emit_operand(a);
                    }
                    self.line(&format!("     (call $def{}_constructor) ;; TODO(layout): alloc", def.0));
                }
            }
            Rvalue::UnionNew { def, ty, variant, args } => {
                // A union value is one heap block `[discriminant: i32][payload...]`, sized to the
                // largest variant so any variant fits. `variant` is the discriminant; allocate,
                // write it at offset 0, then store the payload at the variant's field offsets.
                let layout = self.layouts.union(*ty).and_then(|u| {
                    let size = u.size;
                    u.variants
                        .iter()
                        .find(|v| v.discriminant as usize == *variant)
                        .map(|v| (size, v.fields.iter().map(|f| (f.offset, f.ty)).collect::<Vec<_>>()))
                });
                if let Some((size, fields)) = layout {
                    self.line(&format!("     (i32.const {})", size));
                    self.line(&format!("     (i32.const {}) ;; tag", self.type_tag(*ty, *def)));
                    self.line("     (call $malloc)");
                    self.line("     (local.set $__obj)");
                    self.line("     (local.get $__obj)");
                    self.line(&format!("     (i32.const {}) ;; discriminant", variant));
                    self.line("     (i32.store)");
                    for (i, arg) in args.iter().enumerate() {
                        if let Some(&(off, fty)) = fields.get(i) {
                            self.store_at_obj(off, fty, arg);
                        }
                    }
                    self.line("     (local.get $__obj)");
                } else {
                    self.line(&format!(
                        "     (i32.const 0) ;; union def{} variant {} has no layout",
                        def.0, variant
                    ));
                }
            }
            Rvalue::ArrayLit { elem_ty, elems } => {
                // Array block: `[len: i32][elem0][elem1]...`; the length is the first word (matching
                // `ArrayLen`), elements follow at stride `elem_size`.
                let (esize, _) = scalar_size(self.interner, *elem_ty);
                let size = 4 + esize * (elems.len() as u32);
                self.line(&format!("     (i32.const {})", size));
                self.line(&format!("     (i32.const {}) ;; array tag", ARRAY_TAG));
                self.line("     (call $malloc)");
                self.line("     (local.set $__obj)");
                self.line("     (local.get $__obj)");
                self.line(&format!("     (i32.const {})", elems.len()));
                self.line("     (i32.store) ;; length");
                for (i, e) in elems.iter().enumerate() {
                    self.store_at_obj(4 + esize * (i as u32), *elem_ty, e);
                }
                self.line("     (local.get $__obj)");
            }
            Rvalue::ArrayLen(o) => {
                self.emit_operand(o);
                self.line("     (i32.load) ;; array length is the first word");
            }
            Rvalue::StrLen(o) => {
                self.emit_operand(o);
                self.line("     (call $strlen) ;; strings are NUL-terminated");
            }
            Rvalue::Cast(o, to) => self.emit_cast(o, *to),
        }
    }

    fn emit_cast(&mut self, o: &Operand, to: TypeId) {
        let from = self.operand_ty(o);
        self.emit_operand(o);
        self.emit_numeric_conv(from, to);
    }

    /// Emits a call's arguments, applying implicit numeric widening to each so a narrower argument
    /// (e.g. an `int`/`float` passed to a `double` parameter) matches the callee's WASM signature.
    /// Falls back to a plain push when the callee's parameter types are unknown (imports/intrinsics).
    fn emit_call_args(&mut self, callee: &super::Callee, args: &[Operand]) {
        let params = self.sigs.get(&(callee.def, callee.args.clone())).cloned();
        for (i, a) in args.iter().enumerate() {
            self.emit_operand(a);
            if let Some(pty) = params.as_ref().and_then(|p| p.get(i)) {
                self.emit_numeric_conv(self.operand_ty(a), *pty);
            }
        }
    }

    /// Emits the WASM numeric conversion instruction to turn a value of type `from` (already on the
    /// stack) into type `to`, if their WASM value types differ (a no-op otherwise). Shared by explicit
    /// `Cast` and the implicit widening applied to call arguments.
    fn emit_numeric_conv(&mut self, from: TypeId, to: TypeId) {
        let (fw, tw) = (self.wasm_ty(from), self.wasm_ty(to));
        if fw != tw {
            // Numeric conversions between the four WASM value types. Integer/float conversions carry
            // the signedness of the *integer* side (the target for float→int, the source otherwise);
            // saturating float→int truncation matches C-style cast semantics (no trap on overflow/NaN).
            let (fw, tw) = (fw.as_str(), tw.as_str());
            let int_signed = |ty: TypeId| {
                !matches!(
                    self.interner.kind(self.interner.strip_nullable(ty)),
                    TyKind::Prim(PrimTy::UInt | PrimTy::ULong | PrimTy::Byte)
                )
            };
            let instr = match (fw, tw) {
                ("i32", "i64") => if int_signed(from) { "i64.extend_i32_s" } else { "i64.extend_i32_u" },
                ("i64", "i32") => "i32.wrap_i64",
                ("i32", "f32") => if int_signed(from) { "f32.convert_i32_s" } else { "f32.convert_i32_u" },
                ("i32", "f64") => if int_signed(from) { "f64.convert_i32_s" } else { "f64.convert_i32_u" },
                ("i64", "f32") => if int_signed(from) { "f32.convert_i64_s" } else { "f32.convert_i64_u" },
                ("i64", "f64") => if int_signed(from) { "f64.convert_i64_s" } else { "f64.convert_i64_u" },
                ("f32", "f64") => "f64.promote_f32",
                ("f64", "f32") => "f32.demote_f64",
                ("f32", "i32") => if int_signed(to) { "i32.trunc_sat_f32_s" } else { "i32.trunc_sat_f32_u" },
                ("f64", "i32") => if int_signed(to) { "i32.trunc_sat_f64_s" } else { "i32.trunc_sat_f64_u" },
                ("f32", "i64") => if int_signed(to) { "i64.trunc_sat_f32_s" } else { "i64.trunc_sat_f32_u" },
                ("f64", "i64") => if int_signed(to) { "i64.trunc_sat_f64_s" } else { "i64.trunc_sat_f64_u" },
                _ => "nop",
            };
            self.line(&format!("     ({})", instr));
        }
    }

    fn emit_terminator(&mut self, t: &Terminator) {
        match t {
            Terminator::Goto(b) => self.goto(*b),
            Terminator::If { cond, then_blk, else_blk } => {
                self.emit_operand(cond);
                self.line("     (if (then");
                self.goto(*then_blk);
                self.line("     ) (else");
                self.goto(*else_blk);
                self.line("     ))");
            }
            Terminator::Switch { value, targets, default } => {
                // Lower to a chain of compares; a real br_table needs contiguous keys.
                for (k, b) in targets {
                    self.emit_operand(value);
                    self.line(&format!("     (i32.const {})", k));
                    self.line("     (i32.eq)");
                    self.line("     (if (then");
                    self.goto(*b);
                    self.line("     ))");
                }
                self.goto(*default);
            }
            Terminator::Return(Some(o)) => {
                self.emit_operand(o);
                self.line("     (return)");
            }
            Terminator::Return(None) => self.line("     (return)"),
            Terminator::Unreachable => self.line("     (unreachable)"),
            Terminator::AsyncComplete(_) => self.line("     (unreachable) ;; async in sync fn"),
        }
    }

    /// Terminator emission for async poll segments (completes the task instead of returning).
    fn emit_poll_terminator(&mut self, t: &Terminator) {
        match t {
            Terminator::AsyncComplete(v) => {
                if let Some(parent) = self.async_parent {
                    for (i, decl) in parent.locals.iter().enumerate() {
                        if self.interner.is_reference(decl.ty) {
                            let call = release_call(self.interner, self.layouts, decl.ty);
                            self.line(&format!("     (local.get ${i})"));
                            self.line(&format!("     (call {call})"));
                        }
                    }
                }
                self.line("     (local.get $self)");
                if let Some(v) = v {
                    self.emit_operand(v);
                } else {
                    self.line("     (i32.const 0)");
                }
                self.line("     (call $dream_complete)");
                self.line("     (i32.const 0)");
                self.line("     (return)");
            }
            Terminator::Return(Some(o)) => {
                self.emit_operand(o);
                self.line("     (return)");
            }
            Terminator::Return(None) => self.line("     (return)"),
            _ => {}
        }
    }

    /// Emits `sleep` / `Promise.all|any|race`, leaving a `Future` pointer on the stack.
    fn emit_async_intrinsic(&mut self, kind: &str, args: &[Operand]) {
        use crate::intrinsics;
        match kind {
            intrinsics::SLEEP => {
                self.emit_operand(&args[0]);
                self.line("     (local.set $__scratch)");
                self.line("     (i32.const 56) ;; F_SLOTS");
                self.line("     (i32.const -1)");
                self.line("     (i32.const 1) ;; KIND_HOST");
                self.line("     (call $dream_new_future)");
                self.line("     (local.tee $__obj)");
                self.line("     (local.get $__scratch)");
                self.line("     (call $dream_set_timer)");
                self.line("     (local.get $__obj)");
            }
            intrinsics::PROMISE_ALL => {
                self.emit_operand(&args[0]);
                self.line("     (call $dream_all)");
            }
            intrinsics::PROMISE_ANY | intrinsics::PROMISE_RACE => {
                self.emit_operand(&args[0]);
                self.line("     (call $dream_any)");
            }
            _ => {}
        }
    }

    /// A CFG edge: set the dispatch PC to the target and loop back to re-dispatch.
    fn goto(&mut self, target: super::BlockId) {
        self.line(&format!("     (i32.const {})", target.0));
        self.line("     (local.set $__pc)");
        self.line("     (br $__loop)");
    }

    fn emit_operand(&mut self, op: &Operand) {
        match op {
            Operand::Const(c) => self.emit_const(c),
            Operand::Copy(Place::Local(l)) => self.line(&format!("     (local.get ${})", l.0)),
            Operand::Copy(Place::Global(g)) => self.line(&format!("     (global.get $g{})", g.0)),
            Operand::Copy(Place::Field { base, field }) => {
                if let Some((off, fty)) = self.field_layout(*base, *field) {
                    self.field_addr(*base, off);
                    self.line(&format!("     ({})", self.load_instr(fty)));
                } else {
                    self.line(&format!(
                        "     (local.get ${}) (i32.load) ;; TODO(layout): field {}",
                        base.0, field
                    ));
                }
            }
            Operand::Copy(Place::Index { base, index }) => {
                if let Some(ety) = self.array_elem_ty(*base) {
                    self.elem_addr(*base, ety, index);
                    self.line(&format!("     ({})", self.load_instr(ety)));
                } else {
                    self.line(&format!(
                        "     (local.get ${}) (i32.load) ;; TODO(layout): index",
                        base.0
                    ));
                }
            }
        }
    }

    fn emit_const(&mut self, c: &Const) {
        match c {
            Const::Int(v) => self.line(&format!("     (i32.const {})", v)),
            Const::Long(v) => self.line(&format!("     (i64.const {})", v)),
            Const::Float(v) => self.line(&format!("     (f64.const {})", v)),
            Const::F32(v) => self.line(&format!("     (f32.const {})", v)),
            Const::Bool(v) => self.line(&format!("     (i32.const {})", *v as i32)),
            Const::Char(v) => self.line(&format!("     (i32.const {})", *v as u32)),
            Const::Null => self.line("     (i32.const 0)"),
            Const::Str(s) => match self.strings.get(s) {
                Some(addr) => self.line(&format!("     (i32.const {})", addr)),
                None => self.line("     (i32.const 0) ;; TODO(strings): interned pointer"),
            },
        }
    }

    fn operand_ty(&self, op: &Operand) -> TypeId {
        match op {
            Operand::Copy(Place::Local(l)) => self.func.local_ty(*l),
            Operand::Copy(Place::Field { base, field }) => self
                .field_layout(*base, *field)
                .map(|(_, t)| t)
                .unwrap_or_else(|| self.func.local_ty(*base)),
            Operand::Copy(Place::Index { base, .. }) => {
                self.array_elem_ty(*base).unwrap_or_else(|| self.func.local_ty(*base))
            }
            Operand::Copy(Place::Global(_)) => self.interner.int(),
            Operand::Const(Const::Long(_)) => self.interner.long(),
            Operand::Const(Const::Float(_)) => self.interner.double(),
            Operand::Const(Const::F32(_)) => self.interner.float(),
            Operand::Const(_) => self.interner.int(),
        }
    }

    fn wasm_ty(&self, ty: TypeId) -> String {
        match self.interner.kind(self.interner.strip_nullable(ty)) {
            TyKind::Prim(PrimTy::Double | PrimTy::Long | PrimTy::ULong) => {
                match self.interner.kind(self.interner.strip_nullable(ty)) {
                    TyKind::Prim(PrimTy::Double) => "f64".to_string(),
                    _ => "i64".to_string(),
                }
            }
            TyKind::Prim(PrimTy::Float) => "f32".to_string(),
            TyKind::Void => "i32".to_string(),
            _ => "i32".to_string(),
        }
    }

    fn binop_instr(&self, op: BinOp, ty: TypeId) -> String {
        let w = self.wasm_ty(ty);
        let signed = !matches!(
            self.interner.kind(self.interner.strip_nullable(ty)),
            TyKind::Prim(PrimTy::UInt | PrimTy::ULong | PrimTy::Byte)
        );
        let s = if signed { "_s" } else { "_u" };
        let is_float = w == "f32" || w == "f64";
        match op {
            BinOp::Add => format!("{}.add", w),
            BinOp::Sub => format!("{}.sub", w),
            BinOp::Mul => format!("{}.mul", w),
            BinOp::Div if is_float => format!("{}.div", w),
            BinOp::Div => format!("{}.div{}", w, s),
            BinOp::Rem => format!("{}.rem{}", w, s),
            BinOp::Eq => format!("{}.eq", w),
            BinOp::Ne => format!("{}.ne", w),
            BinOp::Lt if is_float => format!("{}.lt", w),
            BinOp::Lt => format!("{}.lt{}", w, s),
            BinOp::Le if is_float => format!("{}.le", w),
            BinOp::Le => format!("{}.le{}", w, s),
            BinOp::Gt if is_float => format!("{}.gt", w),
            BinOp::Gt => format!("{}.gt{}", w, s),
            BinOp::Ge if is_float => format!("{}.ge", w),
            BinOp::Ge => format!("{}.ge{}", w, s),
            BinOp::And | BinOp::BitAnd => format!("{}.and", w),
            BinOp::Or | BinOp::BitOr => format!("{}.or", w),
            BinOp::BitXor => format!("{}.xor", w),
            BinOp::Shl => format!("{}.shl", w),
            BinOp::Shr => format!("{}.shr{}", w, s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::build::FunctionBuilder;
    use crate::mir::{Place, Rvalue, Terminator};

    #[test]
    fn module_wraps_and_resolves_call_symbols() {
        use crate::mir::Callee;
        use crate::types::DefId;
        let i = TypeInterner::new();

        // fun callee(): int { return 0; }  (def 1)
        let mut cb = FunctionBuilder::new("callee", i.int());
        cb.set_def(DefId(1), vec![]);
        cb.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        let callee = cb.finish();

        // fun caller(): int { return callee(); }  (def 2, calls def 1)
        let mut rb = FunctionBuilder::new("caller", i.int());
        rb.set_def(DefId(2), vec![]);
        let t = rb.new_temp(i.int());
        rb.assign(
            Place::Local(t),
            Rvalue::Call {
                callee: Callee { def: DefId(1), args: vec![], ret: i.int() },
                args: vec![],
            },
        );
        rb.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let caller = rb.finish();

        let mir = crate::mir::Mir { functions: vec![callee, caller], ..Default::default() };
        let wat = emit_module(&mir, &i);
        assert!(wat.starts_with("(module"), "should be wrapped in a module:\n{}", wat);
        assert!(wat.contains("(func $callee"), "callee header:\n{}", wat);
        // The call site resolves to the callee's symbol, not a bare def index.
        assert!(wat.contains("(call $callee)"), "call must resolve to the header symbol:\n{}", wat);
        assert!(wat.contains("(export \"caller\""), "non-instance funcs are exported:\n{}", wat);
    }

    #[test]
    fn instance_functions_get_distinct_symbols() {
        use crate::types::DefId;
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("id", i.int());
        b.set_def(DefId(7), vec![i.int()]);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        let f = b.finish();
        let wat = emit_function(&f, &i);
        // The instance args are encoded into the symbol so monomorphizations stay distinct.
        assert!(wat.contains(&format!("(func $id__{}", i.int().0)), "instance symbol:\n{}", wat);
    }

    #[test]
    fn field_access_uses_layout_offsets_and_widths() {
        use crate::hir::{FieldLayout, LayoutTable, TypeLayout};
        use crate::types::DefId;
        let mut i = TypeInterner::new();
        let def = DefId(3);
        let dbl = i.prim(PrimTy::Double);
        let int = i.int();
        let sty = i.struct_ty(def, vec![]);

        let mut layouts = LayoutTable::default();
        layouts.insert(
            sty,
            TypeLayout {
                name: "S".into(),
                fields: vec![
                    FieldLayout { offset: 0, ty: int, name: "a".into() },
                    FieldLayout { offset: 8, ty: dbl, name: "b".into() },
                ],
                size: 16,
            },
        );

        // fun read(p: S): double { return p.<field 1>; }
        let mut b = FunctionBuilder::new("read", dbl);
        b.set_def(DefId(9), vec![]);
        let p = b.new_param(sty, Some("p".into()));
        let t = b.new_temp(dbl);
        b.assign(
            Place::Local(t),
            Rvalue::Use(Operand::Copy(Place::Field { base: p, field: 1 })),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mir = crate::mir::Mir { functions: vec![b.finish()], layouts, ..Default::default() };
        let wat = emit_program(&mir, &i);
        assert!(wat.contains("(i32.const 8)"), "field 1 sits at byte offset 8:\n{}", wat);
        assert!(wat.contains("(f64.load)"), "a double field loads as f64:\n{}", wat);
    }

    #[test]
    fn new_allocates_and_initializes_fields() {
        use crate::hir::{FieldLayout, LayoutTable, TypeLayout};
        use crate::mir::Rvalue;
        use crate::types::DefId;
        let mut i = TypeInterner::new();
        let def = DefId(5);
        let int = i.int();
        let sty = i.struct_ty(def, vec![]);

        let mut layouts = LayoutTable::default();
        layouts.insert(
            sty,
            TypeLayout {
                name: "S".into(),
                fields: vec![
                    FieldLayout { offset: 0, ty: int, name: "a".into() },
                    FieldLayout { offset: 4, ty: int, name: "b".into() },
                ],
                size: 8,
            },
        );

        // fun make(): S { return S(1, 2); }
        let mut b = FunctionBuilder::new("make", sty);
        b.set_def(DefId(9), vec![]);
        let t = b.new_temp(sty);
        b.assign(
            Place::Local(t),
            Rvalue::New { def, ty: sty, ctor: None, args: vec![Operand::Const(Const::Int(1)), Operand::Const(Const::Int(2))] },
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mir = crate::mir::Mir { functions: vec![b.finish()], layouts, ..Default::default() };
        let wat = emit_program(&mir, &i);
        assert!(wat.contains("(i32.const 8)"), "allocates the struct's data size:\n{}", wat);
        assert!(wat.contains("(call $malloc)"), "constructs via malloc:\n{}", wat);
        assert!(wat.contains("(local.set $__obj)"), "captures the object pointer:\n{}", wat);
        assert!(wat.contains("(i32.store)"), "initializes fields:\n{}", wat);
    }

    #[test]
    fn strings_get_data_segments_and_addresses() {
        use crate::types::DefId;
        let i = TypeInterner::new();
        let str_ty = i.string();
        let mut b = FunctionBuilder::new("hello", str_ty);
        b.set_def(DefId(1), vec![]);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Str("hi".into())))));

        let mir = crate::mir::Mir { functions: vec![b.finish()], ..Default::default() };
        let wat = emit_module(&mir, &i);
        // The runtime constants are interned first (`true`/`false`/`-` then the object-protocol
        // `null`/`<object>`/`[`/`]`/`, `), so the user's "hi" follows at block 1172 / data pointer 1184.
        assert!(wat.contains("(i32.const 1184)"), "string data pointer:\n{}", wat);
        // Its data segment (at the block start) is the heap-object block: header `size=0`, `tag=5`,
        // `ref_count=1`, then the bytes 'h','i', then the NUL terminator.
        assert!(
            wat.contains(
                "(data (i32.const 1172) \"\\00\\00\\00\\00\\05\\00\\00\\00\\01\\00\\00\\00\\68\\69\\00\")"
            ),
            "string data segment:\n{}",
            wat
        );
    }

    #[test]
    fn emit_module_assembles_to_valid_wasm() {
        use crate::hir::{FieldLayout, LayoutTable, TypeLayout};
        use crate::mir::{Callee, MirGlobal, Rvalue};
        use crate::types::DefId;
        let mut i = TypeInterner::new();
        let int = i.int();
        let def = DefId(4);
        let sty = i.struct_ty(def, vec![]);

        let mut layouts = LayoutTable::default();
        layouts.insert(
            sty,
            TypeLayout {
                name: "S".into(),
                fields: vec![FieldLayout { offset: 0, ty: int, name: "a".into() }],
                size: 4,
            },
        );

        // fun helper(): int { return 7; }  (def 1)
        let mut hb = FunctionBuilder::new("helper", int);
        hb.set_def(DefId(1), vec![]);
        hb.terminate(Terminator::Return(Some(Operand::Const(Const::Int(7)))));

        // fun run(): int {
        //   let o = S(helper());   ; allocation + call + field store
        //   g0 = o.x;              ; global write from a field read
        //   return o.x;
        // }
        let mut rb = FunctionBuilder::new("run", int);
        rb.set_def(DefId(2), vec![]);
        let call_t = rb.new_temp(int);
        rb.assign(
            Place::Local(call_t),
            Rvalue::Call { callee: Callee { def: DefId(1), args: vec![], ret: int }, args: vec![] },
        );
        let obj = rb.new_temp(sty);
        rb.assign(
            Place::Local(obj),
            Rvalue::New { def, ty: sty, ctor: None, args: vec![Operand::Copy(Place::Local(call_t))] },
        );
        rb.assign(
            Place::Global(crate::mir::Global(0)),
            Rvalue::Use(Operand::Copy(Place::Field { base: obj, field: 0 })),
        );
        rb.terminate(Terminator::Return(Some(Operand::Copy(Place::Field { base: obj, field: 0 }))));

        let mir = crate::mir::Mir {
            functions: vec![hb.finish(), rb.finish()],
            globals: vec![MirGlobal { id: crate::mir::Global(0), ty: int }],
            layouts,
            ..Default::default()
        };
        let wat = emit_module(&mir, &i);
        // The real gate: the emitted module must assemble to valid WebAssembly.
        wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("emitted module failed to assemble: {}\n{}", e, wat));
    }

    #[test]
    fn emits_arithmetic_function() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("add", i.int());
        let a = b.new_param(i.int(), Some("a".into()));
        let c = b.new_param(i.int(), Some("b".into()));
        let sum = b.new_temp(i.int());
        b.assign(
            Place::Local(sum),
            Rvalue::Binary(BinOp::Add, Operand::Copy(Place::Local(a)), Operand::Copy(Place::Local(c))),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(sum)))));
        let func = b.finish();

        let wat = emit_function(&func, &i);
        assert!(wat.contains("(func $add"), "should emit a function header");
        assert!(wat.contains("i32.add"), "should emit the add instruction:\n{}", wat);
        assert!(wat.contains("(return)"));
        assert!(wat.contains("br_table"));
    }
}
