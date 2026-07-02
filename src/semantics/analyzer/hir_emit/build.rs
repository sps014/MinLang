use super::*;

impl<'a> Analyzer<'a> {
    /// Turns on HIR collection so a top-level variable's initializer expression is captured while it
    /// is analyzed. There is no enclosing function, so there are no locals/blocks — only the top
    /// expression's HIR is wanted. Paired with [`Self::hir_global_init_finish`].
    pub(in crate::semantics::analyzer) fn hir_global_init_begin(&mut self) {
        self.hir.collecting = true;
        self.hir.ok = true;
        self.hir.last = None;
    }

    /// Stores the captured initializer for global `name` (if it was fully representable) and turns
    /// collection back off.
    pub(in crate::semantics::analyzer) fn hir_global_init_finish(&mut self, name: &str) {
        if self.hir.collecting && self.hir.ok {
            if let Some(init) = self.hir.last.take() {
                self.hir.pending_global_inits.insert(name.to_string(), init);
            }
        }
        self.hir.collecting = false;
        self.hir.last = None;
    }

    /// Registers one top-level variable's HIR slot as it is analyzed (in declaration order), so a
    /// *later* global's initializer can resolve an *earlier* global to a [`Binding::Global`]. The
    /// slot `id` must equal the variable's index in [`Analyzer::globals`]. The initializer captured
    /// by [`Self::hir_global_init_finish`] (if representable) is attached to the surfaced [`HGlobal`].
    pub(in crate::semantics::analyzer) fn hir_register_global(&mut self, name: &str, type_str: &str, is_const: bool) {
        let ty = self.type_ctx.lower_str(type_str);
        let id = GlobalId(self.hir.globals.len() as u32);
        self.hir.globals.insert(name.to_string(), (id, ty));
        let init = self.hir.pending_global_inits.shift_remove(name);
        self.hir.global_decls.push(HGlobal {
            id,
            name: name.to_string(),
            ty,
            is_const,
            init,
        });
    }

    /// Builds the [`crate::hir::LayoutTable`] from the analyzed struct and union tables: each struct's
    /// `DefId` maps to its field offsets/sizes, and each union's `DefId` to its per-variant
    /// discriminant + payload offsets, so the backend can lower `obj.field` reads and `new`/variant
    /// construction to concrete loads/stores.
    pub(in crate::semantics::analyzer) fn hir_build_layouts(&mut self) -> crate::hir::LayoutTable {
        use crate::hir::{FieldLayout, LayoutTable, TypeLayout, UnionLayout, UnionVariant};
        // Snapshot field types in declaration order first, so `type_ctx` can be re-borrowed mutably
        // for lowering without aliasing the struct/union-table borrows.
        // Discriminated unions are also registered in the struct table (for tagging/release), but they
        // get a variant-aware layout + `to_string` from the union table below — so exclude them here to
        // avoid a duplicate (empty) struct layout and a duplicate `$<Union>_to_string`.
        let struct_snapshot: Vec<(String, Vec<(String, Type)>)> = self
            .struct_table
            .structs
            .iter()
            .filter(|(name, _)| !self.union_table.contains_key(name.as_str()))
            .map(|(name, info)| {
                let fields =
                    info.fields.iter().map(|(fname, f)| (fname.clone(), f.type_.clone())).collect();
                (name.clone(), fields)
            })
            .collect();
        // (union name, block size, [(variant name, discriminant, [(field name, offset, field type)])]).
        type VariantSnap = (String, i32, Vec<(String, u32, Type)>);
        let union_snapshot: Vec<(String, u32, Vec<VariantSnap>)> = self
            .union_table
            .iter()
            .map(|(name, info)| {
                let variants = info
                    .variants
                    .iter()
                    .map(|v| {
                        let fields = v
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.offset as u32, f.type_.clone()))
                            .collect();
                        (v.name.clone(), v.discriminant, fields)
                    })
                    .collect();
                (name.clone(), info.size as u32, variants)
            })
            .collect();

        let mut layouts = LayoutTable::default();
        for (name, fields) in struct_snapshot {
            // Key by the struct's interned type id (`lower_str` canonicalizes both plain names and
            // mangled generic instances like `Box_int` to `struct_ty(def, args)`), so each
            // monomorphization gets its own layout.
            let ty = self.type_ctx.lower_str(&name);
            let defs: Vec<(String, TypeId)> =
                fields.iter().map(|(fname, t)| (fname.clone(), self.type_ctx.lower(t))).collect();
            layouts.insert(ty, TypeLayout::from_fields(&self.type_ctx.interner, name, defs));
        }
        for (name, size, variants) in union_snapshot {
            let ty = self.type_ctx.lower_str(&name);
            let mut vs = Vec::with_capacity(variants.len());
            for (vname, discriminant, fields) in variants {
                let flds = fields
                    .into_iter()
                    .map(|(fname, offset, t)| FieldLayout {
                        offset,
                        ty: self.type_ctx.lower(&t),
                        name: fname,
                    })
                    .collect();
                vs.push(UnionVariant { name: vname, discriminant, fields: flds });
            }
            layouts.insert_union(ty, UnionLayout { name, variants: vs, size });
        }
        layouts
    }

    /// Collects the module's host/interop imports: every non-intrinsic `extern fun` (top-level or a
    /// class/`extend` static member) becomes an [`HImport`] the backend emits as `(import ...)`.
    /// Overloaded externs share one imported field, so entries are de-duplicated by name.
    pub(in crate::semantics::analyzer) fn hir_build_imports(
        &mut self,
        node: &crate::syntax::nodes::ProgramNode,
    ) -> Vec<HImport> {
        use crate::types::method_fn;
        let mut imports: Vec<HImport> = Vec::new();
        // Each candidate is paired with the name it was *registered* under: top-level externs keep
        // their bare name, while class/`extend` static externs are mangled `{Type}_{method}` (the
        // name the call site resolves to). Using the bare method name for a class extern would fail
        // the def lookup and silently drop the import (its call site then falls back to `$def{N}`).
        let top = node.functions.iter().map(|f| (f, f.name.text.clone()));
        let class_methods = node
            .structs
            .iter()
            .flat_map(|s| s.methods.iter().map(move |m| (m, method_fn(&s.name.text, &m.name.text))));
        let extend_methods = node
            .extends
            .iter()
            .flat_map(|e| e.methods.iter().map(move |m| (m, method_fn(&e.target.text, &m.name.text))));
        for (func, sym_name) in top.chain(class_methods).chain(extend_methods) {
            if !func.is_extern || crate::intrinsics::has_intrinsic_attr(&func.attributes) {
                continue;
            }
            if imports.iter().any(|i| i.name == sym_name) {
                continue;
            }
            // Match the def the call site resolves to, so the emitter's symbol table maps the call
            // onto this import's `$name`. Unregistered externs (should not happen) are skipped.
            let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &sym_name) else {
                continue;
            };
            let (module, field) = extern_import_target(func);
            let params = func
                .parameters
                .iter()
                .map(|p| self.type_ctx.lower(&p.type_))
                .collect();
            let ret = match func.return_type.as_ref() {
                Some(t) if *t != Type::Void => Some(self.type_ctx.lower(t)),
                _ => None,
            };
            imports.push(HImport { def, name: sym_name, module, field, params, ret });
        }
        imports
    }

    /// Collects every `@intrinsic("key")` extern as `(callee DefId, key)`. Unlike host imports these
    /// have no `(import ...)` and no emitted body: their call sites resolve directly to the runtime
    /// helper `$<key>` (`string_alloc`, `char_at`, …) or, for `sleep`, are recognized as an async
    /// intrinsic. Methods are looked up under their mangled `{Type}_{method}` def name (the name the
    /// call site resolves to), matching how they were registered.
    pub(in crate::semantics::analyzer) fn hir_build_intrinsics(
        &mut self,
        node: &crate::syntax::nodes::ProgramNode,
    ) -> Vec<(crate::types::DefId, String)> {
        use crate::types::method_fn;
        let mut out: Vec<(crate::types::DefId, String)> = Vec::new();
        for func in node.functions.iter() {
            if let Some(key) = crate::intrinsics::intrinsic_key(&func.attributes) {
                if let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &func.name.text) {
                    out.push((def, key));
                }
            }
        }
        for s in node.structs.iter() {
            for m in s.methods.iter() {
                if let Some(key) = crate::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&s.name.text, &m.name.text);
                    if let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        for e in node.extends.iter() {
            for m in e.methods.iter() {
                if let Some(key) = crate::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&e.target.text, &m.name.text);
                    if let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        out
    }
}
