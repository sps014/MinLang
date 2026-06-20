use std::io::Error;
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use super::WasmGenerator;

/// The fixed WebAssembly runtime emitted into every module: memory globals plus the
/// `$malloc`/`$free`/`$retain` allocator built on a freelist + bump pointer.
///
/// Block layout while allocated: `[size: i32][ref_count: i32][data...]`; while free:
/// `[size: i32][next_free_ptr: i32]`. Returned pointers refer to `data` (block_start + 8).
const RUNTIME_ALLOCATOR: &str = r#"(global $heap_ptr (mut i32) (i32.const 1024))
(global $free_list_head (mut i32) (i32.const 0))

(func $malloc (param $size i32) (result i32)
    (local $curr i32)
    (local $prev i32)
    (local $next i32)
    (local $block_size i32)
    (local $new_ptr i32)
    ;; round size up to a multiple of 4, then reserve 8 bytes for the header
    local.get $size
    i32.const 3
    i32.add
    i32.const -4
    i32.and
    local.set $size
    local.get $size
    i32.const 8
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
                    ;; ref_count = 1, return data pointer
                    local.get $curr
                    i32.const 4
                    i32.add
                    i32.const 1
                    i32.store
                    local.get $curr
                    i32.const 8
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
    i32.const 1
    i32.store
    local.get $new_ptr
    i32.const 8
    i32.add
)

(func $free (param $ptr i32)
    (local $block_start i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 8
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
"#;

impl<'a> WasmGenerator<'a> {
    /// Builds the memory management runtime: the fixed allocator/string helpers (emitted from
    /// templates) plus the per-type `$release_*` functions generated from the struct table.
    pub fn build_memory_management(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
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

        // Array releases (we need to know which array types are used)
        // For now, we can just generate release functions for basic array types
        self.build_release_func("int[]", None, writer)?;
        self.build_release_func("float[]", None, writer)?;
        self.build_release_func("double[]", None, writer)?;
        self.build_release_func("bool[]", None, writer)?;
        self.build_release_func("string[]", None, writer)?;
        
        for name in self.struct_table.structs.keys() {
            self.build_release_func(&format!("{}[]", name), None, writer)?;
        }

        Ok(())
    }

    fn build_release_func(&self, type_name: &str, struct_info: Option<&crate::lang::semantic_analysis::struct_table::StructInfo>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Replace [] with _array for function name
        let func_name = type_name.replace("[]", "_array");
        
        writer.write_line(&format!("(func $release_{} (param $ptr i32)", func_name));
        writer.indent();
        writer.write_line("(local $ref_count_ptr i32)");
        writer.write_line("(local $new_count i32)");
        if type_name.ends_with("[]") {
            writer.write_line("(local $len i32)");
            writer.write_line("(local $i i32)");
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
        
        // Deep release logic
        if let Some(info) = struct_info {
            for (_, field_info) in &info.fields {
                let field_type = field_info.type_.get_type();
                if self.is_reference_type(&field_type) {
                    let release_func = field_type.replace("[]", "_array").replace("?", "");
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
                let release_func = inner_type.replace("[]", "_array").replace("?", "");
                
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
                
                // Call release on element
                writer.write_line("local.get $ptr");
                writer.write_line("i32.const 4");
                writer.write_line("i32.add"); // skip length
                writer.write_line("local.get $i");
                writer.write_line("i32.const 4");
                writer.write_line("i32.mul");
                writer.write_line("i32.add");
                writer.write_line("i32.load"); // load element pointer
                writer.write_line(&format!("call $release_{}", release_func));
                
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
