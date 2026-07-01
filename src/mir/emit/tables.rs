use super::*;

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
pub(super) fn symbol_table(mir: &crate::mir::Mir) -> HashMap<(DefId, Vec<TypeId>), String> {
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
pub(super) fn signature_table(mir: &crate::mir::Mir) -> HashMap<(DefId, Vec<TypeId>), Vec<TypeId>> {
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
pub(super) fn func_table(mir: &crate::mir::Mir) -> HashMap<(DefId, Vec<TypeId>), usize> {
    mir.functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.def, f.instance.clone()), i))
        .collect()
}

/// The canonical `call_indirect` type name + `(param …)`/`(result …)` WASM types for a function-typed
/// `ty` (nullable stripped). Named by its *WASM* signature (so `fun(int)` and `fun(bool)` share one),
/// which is all `call_indirect` distinguishes. `None` if `ty` is not a function type.
pub(super) fn func_sig(interner: &TypeInterner, ty: TypeId) -> Option<(String, Vec<&'static str>, Option<&'static str>)> {
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
pub(super) fn emit_func_signatures(out: &mut String, interner: &TypeInterner) {
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
pub(super) fn emit_func_table(out: &mut String, mir: &crate::mir::Mir) {
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
pub(super) fn struct_tags(mir: &crate::mir::Mir) -> HashMap<TypeId, i32> {
    mir.layouts
        .structs
        .keys()
        .chain(mir.layouts.unions.keys())
        .enumerate()
        .map(|(i, ty)| (*ty, STRUCT_TAG_BASE + i as i32))
        .collect()
}