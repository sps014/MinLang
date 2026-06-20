use std::io::Error;
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use super::WasmGenerator;

impl<'a> WasmGenerator<'a> {
    /// Builds the memory management functions ($malloc, $free, $retain, $release)
    pub fn build_memory_management(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Global variables for memory management
        // $heap_ptr points to the end of the currently allocated bump memory
        // $free_list_head points to the first free block in the freelist
        writer.write_line("(global $heap_ptr (mut i32) (i32.const 1024))");
        writer.write_line("(global $free_list_head (mut i32) (i32.const 0))");
        writer.write_line("");

        // $malloc: Allocates memory of the given size.
        // Memory Block Layout:
        // [size: i32] [ref_count: i32] [data...]
        // When free, the layout is:
        // [size: i32] [next_free_ptr: i32]
        // The returned pointer points to the start of [data...] (i.e., block_start + 8)
        writer.write_line("(func $malloc (param $size i32) (result i32)");
        writer.indent();
        writer.write_line("(local $curr i32)");
        writer.write_line("(local $prev i32)");
        writer.write_line("(local $next i32)");
        writer.write_line("(local $block_size i32)");
        writer.write_line("(local $new_ptr i32)");
        
        // Ensure size is a multiple of 4 for alignment
        writer.write_line("local.get $size");
        writer.write_line("i32.const 3");
        writer.write_line("i32.add");
        writer.write_line("i32.const -4"); // ~3 in two's complement
        writer.write_line("i32.and");
        writer.write_line("local.set $size");

        // We need 8 extra bytes for the header (size + ref_count)
        writer.write_line("local.get $size");
        writer.write_line("i32.const 8");
        writer.write_line("i32.add");
        writer.write_line("local.set $size");

        // Scan freelist
        writer.write_line("global.get $free_list_head");
        writer.write_line("local.set $curr");
        writer.write_line("i32.const 0");
        writer.write_line("local.set $prev");

        writer.write_line("(block $alloc_done");
        writer.indent();
        writer.write_line("(loop $scan_freelist");
        writer.indent();
        
        // If curr == 0, end of freelist reached
        writer.write_line("local.get $curr");
        writer.write_line("i32.eqz");
        writer.write_line("br_if $alloc_done");

        // Get block size
        writer.write_line("local.get $curr");
        writer.write_line("i32.load");
        writer.write_line("local.set $block_size");

        // If block_size >= size, we found a block!
        writer.write_line("local.get $block_size");
        writer.write_line("local.get $size");
        writer.write_line("i32.ge_s");
        writer.write_line("(if");
        writer.indent();
        writer.write_line("(then");
        writer.indent();
        
        // Remove block from freelist
        writer.write_line("local.get $curr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("i32.load"); // Get next_free_ptr
        writer.write_line("local.set $next");

        writer.write_line("local.get $prev");
        writer.write_line("i32.eqz");
        writer.write_line("(if");
        writer.indent();
        writer.write_line("(then");
        writer.indent();
        writer.write_line("local.get $next");
        writer.write_line("global.set $free_list_head");
        writer.unindent();
        writer.write_line(")");
        writer.write_line("(else");
        writer.indent();
        writer.write_line("local.get $prev");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("local.get $next");
        writer.write_line("i32.store");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        // Initialize ref_count to 1
        writer.write_line("local.get $curr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("i32.const 1");
        writer.write_line("i32.store");

        // Return curr + 8
        writer.write_line("local.get $curr");
        writer.write_line("i32.const 8");
        writer.write_line("i32.add");
        writer.write_line("return");

        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        // Move to next block
        writer.write_line("local.get $curr");
        writer.write_line("local.set $prev");
        writer.write_line("local.get $curr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("i32.load");
        writer.write_line("local.set $curr");
        writer.write_line("br $scan_freelist");

        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        // If we reach here, no suitable block was found. Use bump allocator.
        writer.write_line("global.get $heap_ptr");
        writer.write_line("local.set $new_ptr");

        // Update $heap_ptr
        writer.write_line("global.get $heap_ptr");
        writer.write_line("local.get $size");
        writer.write_line("i32.add");
        writer.write_line("global.set $heap_ptr");

        // Initialize block header
        writer.write_line("local.get $new_ptr");
        writer.write_line("local.get $size");
        writer.write_line("i32.store"); // Store size

        writer.write_line("local.get $new_ptr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("i32.const 1");
        writer.write_line("i32.store"); // Store ref_count = 1

        // Return new_ptr + 8
        writer.write_line("local.get $new_ptr");
        writer.write_line("i32.const 8");
        writer.write_line("i32.add");

        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

        // $free: Inserts a block back into the freelist
        writer.write_line("(func $free (param $ptr i32)");
        writer.indent();
        writer.write_line("(local $block_start i32)");
        
        // If ptr is 0, do nothing
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("br_if 0");

        // block_start = ptr - 8
        writer.write_line("local.get $ptr");
        writer.write_line("i32.const 8");
        writer.write_line("i32.sub");
        writer.write_line("local.set $block_start");

        // Set next_free_ptr = free_list_head
        writer.write_line("local.get $block_start");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("global.get $free_list_head");
        writer.write_line("i32.store");

        // free_list_head = block_start
        writer.write_line("local.get $block_start");
        writer.write_line("global.set $free_list_head");

        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

        // $retain: Increments the reference count
        writer.write_line("(func $retain (param $ptr i32)");
        writer.indent();
        writer.write_line("(local $ref_count_ptr i32)");
        
        // If ptr is 0, do nothing
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("br_if 0");

        // ref_count_ptr = ptr - 4
        writer.write_line("local.get $ptr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.sub");
        writer.write_line("local.set $ref_count_ptr");

        // Increment ref_count
        writer.write_line("local.get $ref_count_ptr");
        writer.write_line("local.get $ref_count_ptr");
        writer.write_line("i32.load");
        writer.write_line("i32.const 1");
        writer.write_line("i32.add");
        writer.write_line("i32.store");

        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

        // Generate type-specific release functions
        self.build_type_specific_releases(writer)?;

        // $strlen: Calculates the length of a null-terminated string
        writer.write_line("(func $strlen (param $ptr i32) (result i32)");
        writer.indent();
        writer.write_line("(local $len i32)");
        writer.write_line("i32.const 0");
        writer.write_line("local.set $len");
        writer.write_line("(block $end");
        writer.indent();
        writer.write_line("(loop $start");
        writer.indent();
        writer.write_line("local.get $ptr");
        writer.write_line("local.get $len");
        writer.write_line("i32.add");
        writer.write_line("i32.load8_u");
        writer.write_line("i32.eqz");
        writer.write_line("br_if $end");
        writer.write_line("local.get $len");
        writer.write_line("i32.const 1");
        writer.write_line("i32.add");
        writer.write_line("local.set $len");
        writer.write_line("br $start");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        writer.write_line("local.get $len");
        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

        // $concat_strings: Concatenates two strings and returns a new allocated string
        writer.write_line("(func $concat_strings (param $str1 i32) (param $str2 i32) (result i32)");
        writer.indent();
        writer.write_line("(local $len1 i32)");
        writer.write_line("(local $len2 i32)");
        writer.write_line("(local $new_ptr i32)");
        writer.write_line("(local $i i32)");
        
        // len1 = strlen(str1)
        writer.write_line("local.get $str1");
        writer.write_line("call $strlen");
        writer.write_line("local.set $len1");
        
        // len2 = strlen(str2)
        writer.write_line("local.get $str2");
        writer.write_line("call $strlen");
        writer.write_line("local.set $len2");
        
        // new_ptr = malloc(len1 + len2 + 1)
        writer.write_line("local.get $len1");
        writer.write_line("local.get $len2");
        writer.write_line("i32.add");
        writer.write_line("i32.const 1");
        writer.write_line("i32.add");
        writer.write_line("call $malloc");
        writer.write_line("local.set $new_ptr");
        
        // Copy str1
        writer.write_line("i32.const 0");
        writer.write_line("local.set $i");
        writer.write_line("(block $end1");
        writer.indent();
        writer.write_line("(loop $start1");
        writer.indent();
        writer.write_line("local.get $i");
        writer.write_line("local.get $len1");
        writer.write_line("i32.eq");
        writer.write_line("br_if $end1");
        
        writer.write_line("local.get $new_ptr");
        writer.write_line("local.get $i");
        writer.write_line("i32.add");
        
        writer.write_line("local.get $str1");
        writer.write_line("local.get $i");
        writer.write_line("i32.add");
        writer.write_line("i32.load8_u");
        
        writer.write_line("i32.store8");
        
        writer.write_line("local.get $i");
        writer.write_line("i32.const 1");
        writer.write_line("i32.add");
        writer.write_line("local.set $i");
        writer.write_line("br $start1");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        
        // Copy str2
        writer.write_line("i32.const 0");
        writer.write_line("local.set $i");
        writer.write_line("(block $end2");
        writer.indent();
        writer.write_line("(loop $start2");
        writer.indent();
        writer.write_line("local.get $i");
        writer.write_line("local.get $len2");
        writer.write_line("i32.eq");
        writer.write_line("br_if $end2");
        
        writer.write_line("local.get $new_ptr");
        writer.write_line("local.get $len1");
        writer.write_line("i32.add");
        writer.write_line("local.get $i");
        writer.write_line("i32.add");
        
        writer.write_line("local.get $str2");
        writer.write_line("local.get $i");
        writer.write_line("i32.add");
        writer.write_line("i32.load8_u");
        
        writer.write_line("i32.store8");
        
        writer.write_line("local.get $i");
        writer.write_line("i32.const 1");
        writer.write_line("i32.add");
        writer.write_line("local.set $i");
        writer.write_line("br $start2");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        
        // Null terminator
        writer.write_line("local.get $new_ptr");
        writer.write_line("local.get $len1");
        writer.write_line("local.get $len2");
        writer.write_line("i32.add");
        writer.write_line("i32.add");
        writer.write_line("i32.const 0");
        writer.write_line("i32.store8");
        
        writer.write_line("local.get $new_ptr");
        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

        // $debug_get_free_list_head: Returns the current free list head
        writer.write_line("(func $debug_get_free_list_head (result i32)");
        writer.indent();
        writer.write_line("global.get $free_list_head");
        writer.unindent();
        writer.write_line(")");
        writer.write_line("");

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
            for (field_name, field_info) in &info.fields {
                let field_type = field_info.type_.get_type();
                if self.is_reference_type(&field_type) {
                    let release_func = field_type.replace("[]", "_array");
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
                let release_func = inner_type.replace("[]", "_array");
                
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
        type_name == "string" || type_name.ends_with("[]") || self.struct_table.get_struct(type_name).is_some()
    }
}
