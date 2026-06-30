use super::WasmGenerator;
use crate::syntax::nodes::types::{method_fn, release_func_suffix, PRIMITIVE_TYPE_NAMES};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use std::io::Error;

/// The minimum heap base address. The heap starts above all string/runtime data, but never below
/// this historical floor, so small programs stay byte-for-byte unchanged.
const MIN_HEAP_BASE: usize = 1024;

/// The fixed WebAssembly runtime emitted into every module: memory globals plus the
/// `$malloc`/`$free`/`$retain` allocator built on a freelist + bump pointer.
///
/// Block layout while allocated: `[size: i32][tag: i32][ref_count: i32][data...]`; while free:
/// `[size: i32][next_free_ptr: i32]`. Returned pointers refer to `data` (block_start + 12), so
/// `ref_count` lives at `ptr - 4`, `tag` at `ptr - 8`, and `size` at `ptr - 12`.
const RUNTIME_ALLOCATOR: &str = include_str!("runtime/allocator.wat");

/// The fixed string runtime: `$strlen`, `$concat_strings`, and the `$debug_get_free_list_head`
/// helper used by tests. These are emitted after the type-specific `$release_*` functions.
const RUNTIME_STRINGS: &str = include_str!("runtime/strings.wat");

impl<'a> WasmGenerator<'a> {
    /// Builds the memory management runtime: the fixed allocator/string helpers (emitted from
    /// templates) plus the per-type `$release_*` functions generated from the struct table.
    pub fn build_memory_management(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Place the heap above all string/runtime-string data (8-byte aligned), never below the
        // historical floor so small programs are byte-for-byte unchanged.
        let heap_base = std::cmp::max(MIN_HEAP_BASE, (self.ctx.next_string_offset + 7) & !7);
        writer.write_line(&format!(
            "(global $heap_ptr (mut i32) (i32.const {}))",
            heap_base
        ));
        writer.write_line("(global $free_list_head (mut i32) (i32.const 0))");
        // Allocator introspection counters (surfaced via the `Debug` intrinsics). `$live_objects`
        // is the number of blocks currently handed out (++ in `$malloc`, -- in `$free`);
        // `$total_allocations` is the monotonic count of every `$malloc` ever made. The globals are
        // always declared so the `$debug_get_*` helpers compile, but they are only updated when
        // allocator instrumentation is enabled (see `debug_alloc`); otherwise they stay 0 and the
        // allocator fast path carries no extra instructions.
        writer.write_line("(global $live_objects (mut i32) (i32.const 0))");
        writer.write_line("(global $total_allocations (mut i32) (i32.const 0))");
        writer.write_line("");
        writer.write_block(&self.allocator_runtime());
        self.build_type_specific_releases(writer)?;
        writer.write_block(RUNTIME_STRINGS);
        Ok(())
    }

    /// The allocator runtime with its debug-counter placeholders resolved. When `debug_alloc` is
    /// on, `$malloc` bumps `$live_objects`/`$total_allocations` and `$free` decrements
    /// `$live_objects`; otherwise the placeholders expand to nothing, so the hot allocation path
    /// is byte-for-byte the same as before instrumentation existed.
    fn allocator_runtime(&self) -> String {
        let (malloc_count, free_count) = if self.ctx.debug_alloc {
            (
                "global.get $live_objects\n    i32.const 1\n    i32.add\n    global.set $live_objects\n    \
                 global.get $total_allocations\n    i32.const 1\n    i32.add\n    global.set $total_allocations",
                "global.get $live_objects\n    i32.const 1\n    i32.sub\n    global.set $live_objects",
            )
        } else {
            ("", "")
        };
        RUNTIME_ALLOCATOR
            .replace(";;@DEBUG_ALLOC_COUNT@", malloc_count)
            .replace(";;@DEBUG_FREE_COUNT@", free_count)
    }

    fn build_type_specific_releases(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // String release
        self.build_release_func("string", None, writer)?;

        // Struct releases
        for (name, struct_info) in &self.struct_table.structs {
            self.build_release_func(name, Some(struct_info), writer)?;
        }

        // Array releases. We emit a `$release_*_array` for every array type the program can
        // actually reference, expanded to all nested levels (e.g. `int[][]` requires both
        // `int_array_array` and `int_array`). The primitive and struct arrays are always
        // included so simple programs and the object protocol's dispatch keep working.
        let mut array_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for prim in PRIMITIVE_TYPE_NAMES {
            array_types.insert(format!("{}[]", prim));
        }
        for name in self.struct_table.structs.keys() {
            array_types.insert(format!("{}[]", name));
        }
        self.collect_used_array_types(&mut array_types);

        for array_type in &array_types {
            self.build_release_func(array_type, None, writer)?;
        }

        Ok(())
    }

    /// Scans the program (every function's locals/params, struct fields, and function
    /// signatures) for array-typed values and records each one - plus every nested array
    /// level it contains - into `set`, normalized so the names match `release_func_suffix`
    /// (all `?` markers dropped). Generic and otherwise non-emittable element types are skipped.
    fn collect_used_array_types(&self, set: &mut std::collections::BTreeSet<String>) {
        for table in self.symbol_map.values() {
            if let Ok(vars) = self.get_local_variables(table) {
                for ty in vars.values() {
                    self.add_array_levels(&ty.get_type(), set);
                }
            }
        }
        for struct_info in self.struct_table.structs.values() {
            for field in struct_info.fields.values() {
                self.add_array_levels(&field.type_.get_type(), set);
            }
        }
        for func in self.syntax_tree.get_root().functions.iter() {
            for param in &func.parameters {
                self.add_array_levels(&param.type_.get_type(), set);
            }
            if let Some(ret) = &func.return_type {
                self.add_array_levels(&ret.get_type(), set);
            }
        }
    }

    /// Adds `type_str` and each of its nested array levels to `set` when the element base is a
    /// type we can release (primitive, `object`, or a known struct). Nullable markers are
    /// stripped so the generated function names match the call sites in `emit_release`.
    fn add_array_levels(&self, type_str: &str, set: &mut std::collections::BTreeSet<String>) {
        let mut cur = self.resolve_type(type_str).replace('?', "");
        if !cur.ends_with("[]") {
            return;
        }
        let base = cur.trim_end_matches("[]");
        let is_emittable = PRIMITIVE_TYPE_NAMES.contains(&base)
            || base == "object"
            || self.struct_table.get_struct(base).is_some();
        if !is_emittable {
            return;
        }
        while cur.ends_with("[]") {
            set.insert(cur.clone());
            cur.truncate(cur.len() - 2);
        }
    }

    fn build_release_func(
        &self,
        type_name: &str,
        struct_info: Option<&crate::semantics::struct_table::StructInfo>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // Map the type name to its `$release_*` suffix (arrays -> `_array`, `?` dropped).
        // `type_name` here is already normalized to drop `?`, so this matches the call sites.
        let func_name = release_func_suffix(type_name);

        writer.write_line(&format!("(func $release_{} (param $ptr i32)", func_name));
        writer.indent();
        writer.write_line("(local $ref_count_ptr i32)");
        writer.write_line("(local $new_count i32)");
        if type_name.ends_with("[]") {
            writer.write_line("(local $len i32)");
            writer.write_line("(local $i i32)");
            writer.write_line("(local $elem i32)");
        }

        // If ptr is 0, do nothing
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("br_if 0");

        // ref_count_ptr = ptr - 4
        writer.write_line("local.get $ptr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.sub");
        writer.write_line("local.set $ref_count_ptr");

        // Decrement ref_count
        writer.write_line("local.get $ref_count_ptr");
        writer.write_line("i32.load");
        writer.write_line("i32.const 1");
        writer.write_line("i32.sub");
        writer.write_line("local.set $new_count");

        writer.write_line("local.get $ref_count_ptr");
        writer.write_line("local.get $new_count");
        writer.write_line("i32.store");

        // If new_count == 0, deep release and free
        writer.write_line("local.get $new_count");
        writer.write_line("i32.eqz");
        writer.write_line("(if");
        writer.indent();
        writer.write_line("(then");
        writer.indent();

        // User-defined destructor: when the last reference is released, run `$Struct_del(ptr)`
        // while the fields are still valid, before releasing them and freeing the block.
        // The destructor body retains/releases its own `this` parameter (net zero), so the
        // refcount is first pinned to 1; this keeps that internal release from dropping the
        // count back to 0 and re-entering this release function.
        if struct_info.is_some() {
            let drop_name = method_fn(type_name, crate::syntax::nodes::types::DESTRUCTOR_NAME);
            if self.function_table.get_function(&drop_name).is_ok() {
                writer.write_line("local.get $ref_count_ptr");
                writer.write_line("i32.const 1");
                writer.write_line("i32.store");
                writer.write_line("local.get $ptr");
                writer.write_line(&format!("call ${}", drop_name));
            }
        }

        // Deep release logic
        if let Some(union) = self.unions.get(type_name).cloned() {
            // A discriminated union overlaps its variants' payloads, so only the *active*
            // variant's reference fields are valid: switch on the discriminant (offset 0) and
            // release just those.
            for variant in &union.variants {
                let ref_fields: Vec<&crate::semantics::union_table::UnionFieldInfo> = variant
                    .fields
                    .iter()
                    .filter(|f| {
                        self.is_reference_type(crate::syntax::nodes::types::strip_nullable(
                            &f.type_.get_type(),
                        ))
                    })
                    .collect();
                if ref_fields.is_empty() {
                    continue;
                }
                writer.write_line("local.get $ptr");
                writer.write_line("i32.load"); // discriminant at offset 0
                writer.write_line(&format!("i32.const {}", variant.discriminant));
                writer.write_line("i32.eq");
                writer.write_line("(if");
                writer.indent();
                writer.write_line("(then");
                writer.indent();
                for f in ref_fields {
                    writer.write_line("local.get $ptr");
                    if f.offset > 0 {
                        writer.write_line(&format!("i32.const {}", f.offset));
                        writer.write_line("i32.add");
                    }
                    writer.write_line("i32.load"); // load the field pointer
                    self.emit_release(&f.type_.get_type(), writer);
                }
                writer.unindent();
                writer.write_line(")");
                writer.unindent();
                writer.write_line(")");
            }
        } else if let Some(info) = struct_info {
            for field_info in info.fields.values() {
                let field_type = field_info.type_.get_type();
                if self.is_reference_type(&field_type) {
                    let release_func = release_func_suffix(&field_type);
                    writer.write_line("local.get $ptr");
                    if field_info.offset > 0 {
                        writer.write_line(&format!("i32.const {}", field_info.offset));
                        writer.write_line("i32.add");
                    }
                    writer.write_line("i32.load"); // load the pointer
                    writer.write_line(&format!("call $release_{}", release_func));
                }
            }
        } else if let Some(inner_type) = type_name.strip_suffix("[]") {
            if self.is_reference_type(inner_type) {
                let release_func = release_func_suffix(inner_type);

                // Get length
                writer.write_line("local.get $ptr");
                writer.write_line("i32.load");
                writer.write_line("local.set $len");

                // Loop through elements
                writer.write_line("i32.const 0");
                writer.write_line("local.set $i");

                writer.write_line("(block $loop_end");
                writer.indent();
                writer.write_line("(loop $loop_start");
                writer.indent();

                writer.write_line("local.get $i");
                writer.write_line("local.get $len");
                writer.write_line("i32.ge_s");
                writer.write_line("br_if $loop_end");

                // Load the element pointer (slots past `count` are null in a grown buffer).
                writer.write_line("local.get $ptr");
                writer.write_line("i32.const 4");
                writer.write_line("i32.add"); // skip length
                writer.write_line("local.get $i");
                writer.write_line("i32.const 4");
                writer.write_line("i32.mul");
                writer.write_line("i32.add");
                writer.write_line("i32.load"); // load element pointer
                writer.write_line("local.set $elem");

                // Only release non-null elements.
                writer.write_line("local.get $elem");
                writer.write_line("(if");
                writer.indent();
                writer.write_line("(then");
                writer.indent();
                writer.write_line("local.get $elem");
                writer.write_line(&format!("call $release_{}", release_func));
                writer.unindent();
                writer.write_line(")");
                writer.unindent();
                writer.write_line(")");

                writer.write_line("local.get $i");
                writer.write_line("i32.const 1");
                writer.write_line("i32.add");
                writer.write_line("local.set $i");
                writer.write_line("br $loop_start");

                writer.unindent();
                writer.write_line(")");
                writer.unindent();
                writer.write_line(")");
            }
        }

        // Finally, free the block
        writer.write_line("local.get $ptr");
        writer.write_line("call $free");

        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

        Ok(())
    }

    pub fn is_reference_type(&self, type_name: &str) -> bool {
        self.struct_table.is_reference_type(type_name)
    }
}
