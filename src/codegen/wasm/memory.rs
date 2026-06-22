use std::io::Error;
use crate::syntax::nodes::types::release_func_suffix;
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use super::WasmGenerator;

/// The fixed WebAssembly runtime emitted into every module: memory globals plus the
/// `$malloc`/`$free`/`$retain` allocator built on a freelist + bump pointer.
///
/// Block layout while allocated: `[size: i32][tag: i32][ref_count: i32][data...]`; while free:
/// `[size: i32][next_free_ptr: i32]`. Returned pointers refer to `data` (block_start + 12), so
/// `ref_count` lives at `ptr - 4`, `tag` at `ptr - 8`, and `size` at `ptr - 12`.
const RUNTIME_ALLOCATOR: &str = r#"(func $malloc (param $size i32) (param $tag i32) (result i32)
    (local $curr i32)
    (local $prev i32)
    (local $next i32)
    (local $block_size i32)
    (local $new_ptr i32)
    ;; round size up to a multiple of 4, then reserve 12 bytes for the header
    local.get $size
    i32.const 3
    i32.add
    i32.const -4
    i32.and
    local.set $size
    local.get $size
    i32.const 12
    i32.add
    local.set $size
    ;; scan the freelist for a large-enough block
    global.get $free_list_head
    local.set $curr
    i32.const 0
    local.set $prev
    (block $alloc_done
        (loop $scan_freelist
            local.get $curr
            i32.eqz
            br_if $alloc_done
            local.get $curr
            i32.load
            local.set $block_size
            local.get $block_size
            local.get $size
            i32.ge_s
            (if
                (then
                    ;; unlink this block from the freelist
                    local.get $curr
                    i32.const 4
                    i32.add
                    i32.load
                    local.set $next
                    local.get $prev
                    i32.eqz
                    (if
                        (then
                            local.get $next
                            global.set $free_list_head
                        )
                        (else
                            local.get $prev
                            i32.const 4
                            i32.add
                            local.get $next
                            i32.store
                        )
                    )
                    ;; tag at block+4
                    local.get $curr
                    i32.const 4
                    i32.add
                    local.get $tag
                    i32.store
                    ;; ref_count = 1 at block+8
                    local.get $curr
                    i32.const 8
                    i32.add
                    i32.const 1
                    i32.store
                    ;; return data pointer (block + 12)
                    local.get $curr
                    i32.const 12
                    i32.add
                    return
                )
            )
            local.get $curr
            local.set $prev
            local.get $curr
            i32.const 4
            i32.add
            i32.load
            local.set $curr
            br $scan_freelist
        )
    )
    ;; no free block fit: bump-allocate fresh memory
    global.get $heap_ptr
    local.set $new_ptr
    global.get $heap_ptr
    local.get $size
    i32.add
    global.set $heap_ptr
    local.get $new_ptr
    local.get $size
    i32.store
    local.get $new_ptr
    i32.const 4
    i32.add
    local.get $tag
    i32.store
    local.get $new_ptr
    i32.const 8
    i32.add
    i32.const 1
    i32.store
    local.get $new_ptr
    i32.const 12
    i32.add
)

(func $free (param $ptr i32)
    (local $block_start i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 12
    i32.sub
    local.set $block_start
    local.get $block_start
    i32.const 4
    i32.add
    global.get $free_list_head
    i32.store
    local.get $block_start
    global.set $free_list_head
)

(func $retain (param $ptr i32)
    (local $ref_count_ptr i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 4
    i32.sub
    local.set $ref_count_ptr
    local.get $ref_count_ptr
    local.get $ref_count_ptr
    i32.load
    i32.const 1
    i32.add
    i32.store
)

(func $object_tag (param $ptr i32) (result i32)
    local.get $ptr
    i32.eqz
    (if (result i32)
        (then i32.const 0)
        (else
            local.get $ptr
            i32.const 8
            i32.sub
            i32.load
        )
    )
)

(func $release_generic (param $ptr i32)
    (local $ref_count_ptr i32)
    (local $new_count i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 4
    i32.sub
    local.set $ref_count_ptr
    local.get $ref_count_ptr
    i32.load
    i32.const 1
    i32.sub
    local.set $new_count
    local.get $ref_count_ptr
    local.get $new_count
    i32.store
    local.get $new_count
    i32.eqz
    (if (then
        local.get $ptr
        call $free
    ))
)
"#;

/// The fixed string runtime: `$strlen`, `$concat_strings`, and the `$debug_get_free_list_head`
/// helper used by tests. These are emitted after the type-specific `$release_*` functions.
const RUNTIME_STRINGS: &str = r#"(func $strlen (param $ptr i32) (result i32)
    (local $len i32)
    i32.const 0
    local.set $len
    (block $end
        (loop $start
            local.get $ptr
            local.get $len
            i32.add
            i32.load8_u
            i32.eqz
            br_if $end
            local.get $len
            i32.const 1
            i32.add
            local.set $len
            br $start
        )
    )
    local.get $len
)

(func $concat_strings (param $str1 i32) (param $str2 i32) (result i32)
    (local $len1 i32)
    (local $len2 i32)
    (local $new_ptr i32)
    (local $i i32)
    local.get $str1
    call $strlen
    local.set $len1
    local.get $str2
    call $strlen
    local.set $len2
    local.get $len1
    local.get $len2
    i32.add
    i32.const 1
    i32.add
    i32.const 5
    call $malloc
    local.set $new_ptr
    i32.const 0
    local.set $i
    (block $end1
        (loop $start1
            local.get $i
            local.get $len1
            i32.eq
            br_if $end1
            local.get $new_ptr
            local.get $i
            i32.add
            local.get $str1
            local.get $i
            i32.add
            i32.load8_u
            i32.store8
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $start1
        )
    )
    i32.const 0
    local.set $i
    (block $end2
        (loop $start2
            local.get $i
            local.get $len2
            i32.eq
            br_if $end2
            local.get $new_ptr
            local.get $len1
            i32.add
            local.get $i
            i32.add
            local.get $str2
            local.get $i
            i32.add
            i32.load8_u
            i32.store8
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $start2
        )
    )
    local.get $new_ptr
    local.get $len1
    local.get $len2
    i32.add
    i32.add
    i32.const 0
    i32.store8
    local.get $new_ptr
)

(func $debug_get_free_list_head (result i32)
    global.get $free_list_head
)

(func $string_eq (param $a i32) (param $b i32) (result i32)
    (local $ca i32)
    (local $cb i32)
    ;; identical pointers (covers the both-null case) are trivially equal
    local.get $a
    local.get $b
    i32.eq
    if
        i32.const 1
        return
    end
    ;; a null pointer can only equal another null pointer (handled above)
    local.get $a
    i32.eqz
    if
        i32.const 0
        return
    end
    local.get $b
    i32.eqz
    if
        i32.const 0
        return
    end
    (block $done
        (loop $cmp
            local.get $a
            i32.load8_u
            local.set $ca
            local.get $b
            i32.load8_u
            local.set $cb
            local.get $ca
            local.get $cb
            i32.ne
            if
                i32.const 0
                return
            end
            local.get $ca
            i32.eqz
            if
                i32.const 1
                return
            end
            local.get $a
            i32.const 1
            i32.add
            local.set $a
            local.get $b
            i32.const 1
            i32.add
            local.set $b
            br $cmp
        )
    )
    i32.const 0
)

(func $char_at (param $ptr i32) (param $i i32) (result i32)
    local.get $ptr
    local.get $i
    i32.add
    i32.load8_u
)

(func $string_alloc (param $n i32) (result i32)
    (local $p i32)
    ;; n data bytes + 1 null terminator
    local.get $n
    i32.const 1
    i32.add
    i32.const 5
    call $malloc
    local.set $p
    ;; write the null terminator at [n]; the n data bytes are filled by the caller via $string_set
    local.get $p
    local.get $n
    i32.add
    i32.const 0
    i32.store8
    local.get $p
)

(func $string_set (param $ptr i32) (param $i i32) (param $c i32)
    local.get $ptr
    local.get $i
    i32.add
    local.get $c
    i32.store8
)
"#;

impl<'a> WasmGenerator<'a> {
    /// Builds the memory management runtime: the fixed allocator/string helpers (emitted from
    /// templates) plus the per-type `$release_*` functions generated from the struct table.
    pub fn build_memory_management(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Place the heap above all string/runtime-string data (8-byte aligned), never below the
        // historical 1024-byte base so small programs are byte-for-byte unchanged.
        let heap_base = std::cmp::max(1024, (self.ctx.next_string_offset + 7) & !7);
        writer.write_line(&format!("(global $heap_ptr (mut i32) (i32.const {}))", heap_base));
        writer.write_line("(global $free_list_head (mut i32) (i32.const 0))");
        writer.write_line("");
        writer.write_block(RUNTIME_ALLOCATOR);
        self.build_type_specific_releases(writer)?;
        writer.write_block(RUNTIME_STRINGS);
        Ok(())
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
        for prim in ["int", "float", "double", "bool", "char", "string"] {
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
        let is_emittable = matches!(base, "int" | "float" | "double" | "bool" | "char" | "string" | "object")
            || self.struct_table.get_struct(base).is_some();
        if !is_emittable {
            return;
        }
        while cur.ends_with("[]") {
            set.insert(cur.clone());
            cur.truncate(cur.len() - 2);
        }
    }

    fn build_release_func(&self, type_name: &str, struct_info: Option<&crate::semantics::struct_table::StructInfo>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
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

        // User-defined destructor: when the last reference is released, run `$Struct_drop(ptr)`
        // while the fields are still valid, before releasing them and freeing the block.
        // The destructor body retains/releases its own `this` parameter (net zero), so the
        // refcount is first pinned to 1; this keeps that internal release from dropping the
        // count back to 0 and re-entering this release function.
        if struct_info.is_some() {
            let drop_name = format!("{}_drop", type_name);
            if self.function_table.get_function(&drop_name).is_ok() {
                writer.write_line("local.get $ref_count_ptr");
                writer.write_line("i32.const 1");
                writer.write_line("i32.store");
                writer.write_line("local.get $ptr");
                writer.write_line(&format!("call ${}", drop_name));
            }
        }

        // Deep release logic
        if let Some(info) = struct_info {
            for (_, field_info) in &info.fields {
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
        } else if type_name.ends_with("[]") {
            let inner_type = &type_name[..type_name.len() - 2];
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
