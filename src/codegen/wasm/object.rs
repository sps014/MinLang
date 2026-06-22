use std::io::Error;
use crate::syntax::nodes::{ExpressionNode, FunctionNode};
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use crate::semantics::struct_table::StructInfo;
use super::WasmGenerator;

/// Runtime type tags stored in each heap block's header. Reference types carry their tag in
/// the block they already own; primitives are boxed into a small tagged block.
pub const TAG_INT: i32 = 1;
pub const TAG_FLOAT: i32 = 2;
pub const TAG_DOUBLE: i32 = 3;
pub const TAG_BOOL: i32 = 4;
pub const TAG_STRING: i32 = 5;
pub const TAG_ARRAY: i32 = 6;
pub const TAG_CHAR: i32 = 7;
/// Structs are assigned consecutive tags starting here, ordered by sorted struct name.
pub const TAG_STRUCT_BASE: i32 = 8;

/// Element types for which array `to_string`/`hash_code` helpers are generated.
const PRIMITIVE_ARRAY_ELEMENTS: [&str; 6] = ["int", "float", "double", "bool", "char", "string"];

/// The fixed object-protocol runtime that does not depend on the user program: boxing /
/// unboxing of primitives, primitive hashers, and `$int_to_string` (digit extraction).
const OBJECT_RUNTIME_FIXED: &str = r#"(func $box_int (param $v i32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 1
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store
    local.get $p
)
(func $box_float (param $v f32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 2
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    f32.store
    local.get $p
)
(func $box_double (param $v f64) (result i32)
    (local $p i32)
    i32.const 8
    i32.const 3
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    f64.store
    local.get $p
)
(func $box_bool (param $v i32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 4
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store
    local.get $p
)
(func $unbox_int (param $p i32) (result i32)
    local.get $p
    i32.load
)
(func $unbox_float (param $p i32) (result f32)
    local.get $p
    f32.load
)
(func $unbox_double (param $p i32) (result f64)
    local.get $p
    f64.load
)
(func $unbox_bool (param $p i32) (result i32)
    local.get $p
    i32.load
)
(func $hash_int (param $v i32) (result i32)
    local.get $v
)
(func $hash_bool (param $v i32) (result i32)
    local.get $v
)
(func $hash_float (param $v f32) (result i32)
    local.get $v
    i32.reinterpret_f32
)
(func $hash_double (param $v f64) (result i32)
    local.get $v
    f32.demote_f64
    i32.reinterpret_f32
)
(func $hash_string (param $p i32) (result i32)
    (local $h i32)
    (local $i i32)
    (local $c i32)
    i32.const -2128831035
    local.set $h
    i32.const 0
    local.set $i
    (block $done
        (loop $scan
            local.get $p
            local.get $i
            i32.add
            i32.load8_u
            local.set $c
            local.get $c
            i32.eqz
            br_if $done
            local.get $h
            local.get $c
            i32.xor
            local.set $h
            local.get $h
            i32.const 16777619
            i32.mul
            local.set $h
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $scan
        )
    )
    local.get $h
)
(func $int_to_string (param $v i32) (result i32)
    (local $p i32)
    (local $i i32)
    (local $neg i32)
    (local $start i32)
    (local $end i32)
    (local $tmp i32)
    (local $digit i32)
    i32.const 16
    i32.const 5
    call $malloc
    local.set $p
    local.get $v
    i32.eqz
    (if (then
        local.get $p
        i32.const 48
        i32.store8
        local.get $p
        i32.const 1
        i32.add
        i32.const 0
        i32.store8
        local.get $p
        return
    ))
    i32.const 0
    local.set $neg
    local.get $v
    i32.const 0
    i32.lt_s
    (if (then
        i32.const 1
        local.set $neg
        i32.const 0
        local.get $v
        i32.sub
        local.set $v
    ))
    i32.const 0
    local.set $i
    (block $gen_done
        (loop $gen
            local.get $v
            i32.eqz
            br_if $gen_done
            local.get $v
            i32.const 10
            i32.rem_s
            local.set $digit
            local.get $p
            local.get $i
            i32.add
            local.get $digit
            i32.const 48
            i32.add
            i32.store8
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            local.get $v
            i32.const 10
            i32.div_s
            local.set $v
            br $gen
        )
    )
    local.get $neg
    (if (then
        local.get $p
        local.get $i
        i32.add
        i32.const 45
        i32.store8
        local.get $i
        i32.const 1
        i32.add
        local.set $i
    ))
    local.get $p
    local.get $i
    i32.add
    i32.const 0
    i32.store8
    i32.const 0
    local.set $start
    local.get $i
    i32.const 1
    i32.sub
    local.set $end
    (block $rev_done
        (loop $rev
            local.get $start
            local.get $end
            i32.ge_s
            br_if $rev_done
            local.get $p
            local.get $start
            i32.add
            i32.load8_u
            local.set $tmp
            local.get $p
            local.get $start
            i32.add
            local.get $p
            local.get $end
            i32.add
            i32.load8_u
            i32.store8
            local.get $p
            local.get $end
            i32.add
            local.get $tmp
            i32.store8
            local.get $start
            i32.const 1
            i32.add
            local.set $start
            local.get $end
            i32.const 1
            i32.sub
            local.set $end
            br $rev
        )
    )
    local.get $p
)
(func $box_char (param $v i32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 7
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store
    local.get $p
)
(func $unbox_char (param $p i32) (result i32)
    local.get $p
    i32.load
)
(func $char_to_string (param $v i32) (result i32)
    (local $p i32)
    i32.const 2
    i32.const 5
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store8
    local.get $p
    i32.const 1
    i32.add
    i32.const 0
    i32.store8
    local.get $p
)
"#;

impl<'a> WasmGenerator<'a> {
    /// Returns true for the boxable scalar primitives.
    pub fn is_primitive_name(name: &str) -> bool {
        matches!(name, "int" | "float" | "double" | "bool" | "char")
    }

    /// Normalizes a type name for value rendering: enum types are `i32`s at runtime, so they
    /// collapse to `int`; everything else is returned unchanged.
    pub fn enum_or_int(&self, name: &str) -> String {
        if self.enums.contains_key(name) {
            "int".to_string()
        } else {
            name.to_string()
        }
    }

    /// Struct names in a stable (sorted) order; their position determines their runtime tag.
    pub fn sorted_struct_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.struct_table.structs.keys().cloned().collect();
        names.sort();
        names
    }

    /// Returns the runtime type tag for a type name (after stripping nullable/array suffixes).
    pub fn type_tag(&self, type_name: &str) -> i32 {
        let base = strip_nullable(type_name);
        if base.ends_with("[]") {
            return TAG_ARRAY;
        }
        match base {
            "int" => TAG_INT,
            "float" => TAG_FLOAT,
            "double" => TAG_DOUBLE,
            "bool" => TAG_BOOL,
            "char" => TAG_CHAR,
            "string" => TAG_STRING,
            "object" => 0,
            _ => {
                match self.sorted_struct_names().iter().position(|n| n == base) {
                    Some(i) => TAG_STRUCT_BASE + i as i32,
                    None => 0,
                }
            }
        }
    }

    /// Fields of a struct ordered by their byte offset (i.e. declaration order).
    pub(crate) fn sorted_fields<'b>(info: &'b StructInfo) -> Vec<(String, &'b crate::semantics::struct_table::StructFieldInfo)> {
        let mut fields: Vec<(String, &crate::semantics::struct_table::StructFieldInfo)> =
            info.fields.iter().map(|(k, v)| (k.clone(), v)).collect();
        fields.sort_by_key(|(_, f)| f.offset);
        fields
    }

    /// Interns a runtime-only string literal, returning its data-segment offset (the data
    /// pointer, with the block header living just before it).
    pub fn intern_runtime_string(&mut self, content: &str) -> usize {
        if let Some(&offset) = self.ctx.runtime_strings.get(content) {
            return offset;
        }
        let offset = self.ctx.next_string_offset;
        self.ctx.runtime_strings.insert(content.to_string(), offset);
        self.ctx.next_string_offset += content.len() + 1 + super::HEAP_HEADER_SIZE;
        offset
    }

    /// Looks up an already-interned runtime string offset.
    fn rstr(&self, content: &str) -> usize {
        *self.ctx.runtime_strings.get(content)
            .unwrap_or_else(|| panic!("runtime string not interned: {:?}", content))
    }

    /// The `(prefix, field-labels, suffix)` literal pieces used by a struct's default
    /// `to_string`. Field labels are in declaration (offset) order.
    fn struct_to_string_pieces(info: &StructInfo) -> (String, Vec<String>, String) {
        let prefix = format!("{} {{ ", info.name);
        let labels = Self::sorted_fields(info)
            .iter()
            .enumerate()
            .map(|(i, (name, _))| if i == 0 { format!("{}: ", name) } else { format!(", {}: ", name) })
            .collect();
        (prefix, labels, " }".to_string())
    }

    /// Interns every runtime string the object protocol needs (primitive labels plus each
    /// struct's default `to_string` pieces). Call once, after user strings are collected.
    pub fn register_object_runtime_strings(&mut self) {
        for s in ["true", "false", "null", "<object>", "-", "[", "]", ", ", ""] {
            self.intern_runtime_string(s);
        }
        let infos: Vec<StructInfo> = self.struct_table.structs.values().cloned().collect();
        for info in infos {
            let (prefix, labels, suffix) = Self::struct_to_string_pieces(&info);
            self.intern_runtime_string(&prefix);
            for label in labels {
                self.intern_runtime_string(&label);
            }
            self.intern_runtime_string(&suffix);
        }
        // Every enum variant name, returned by `EnumValue.name()`.
        let member_names: Vec<String> = self.enums.values()
            .flat_map(|members| members.keys().cloned())
            .collect();
        for name in member_names {
            self.intern_runtime_string(&name);
        }
    }

    /// True if the struct provides its own `@override` implementation of `method`
    /// (`to_string` / `hash_code`); otherwise a default is generated.
    fn has_protocol_override(&self, struct_name: &str, method: &str) -> bool {
        self.function_table.get_function(&format!("{}_{}", struct_name, method)).is_ok()
    }

    /// Emits one `$enum_name_<Enum>(i32) -> i32` lookup per declared enum, returning the
    /// interned variant-name string for a value (or the empty string if none matches, which
    /// only happens for out-of-range values produced by an `int` -> enum cast).
    pub fn build_enum_runtime(&self, writer: &mut IndentedTextWriter) {
        let fallback = self.rstr("");
        let mut enum_names: Vec<&String> = self.enums.keys().collect();
        enum_names.sort();
        for enum_name in enum_names {
            writer.write_line(&format!("(func $enum_name_{} (param $v i32) (result i32)", enum_name));
            writer.indent();
            let mut entries: Vec<(&String, &i32)> = self.enums[enum_name].iter().collect();
            entries.sort_by_key(|(_, value)| **value);
            for (member, value) in entries {
                let strptr = self.rstr(member);
                writer.write_line("local.get $v");
                writer.write_line(&format!("i32.const {}", value));
                writer.write_line("i32.eq");
                writer.write_line("if");
                writer.indent();
                writer.write_line(&format!("i32.const {}", strptr));
                writer.write_line("return");
                writer.unindent();
                writer.write_line("end");
            }
            writer.write_line(&format!("i32.const {}", fallback));
            writer.unindent();
            writer.write_line(")");
        }
    }

    /// Emits the entire object-protocol runtime into the module.
    pub fn build_object_runtime(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_block(OBJECT_RUNTIME_FIXED);
        self.build_bool_to_string(writer);
        self.build_float_double_to_string(writer);
        self.build_struct_protocol_defaults(writer)?;
        self.build_array_protocol_helpers(writer)?;
        self.build_object_to_string(writer);
        self.build_object_hash_code(writer);
        self.build_print_object(writer);
        self.build_release_object(writer);
        Ok(())
    }

    fn build_bool_to_string(&self, writer: &mut IndentedTextWriter) {
        let t = self.rstr("true");
        let f = self.rstr("false");
        writer.write_line("(func $bool_to_string (param $v i32) (result i32)");
        writer.indent();
        writer.write_line("local.get $v");
        writer.write_line("(if (result i32)");
        writer.indent();
        writer.write_line(&format!("(then i32.const {})", t));
        writer.write_line(&format!("(else i32.const {})", f));
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
    }

    fn build_float_double_to_string(&self, writer: &mut IndentedTextWriter) {
        let minus = self.rstr("-");
        // double_to_string: integer part + '.' + 6 fractional digits (limited precision).
        writer.write_block(&format!(r#"(func $double_to_string (param $v f64) (result i32)
    (local $ip i32)
    (local $frac f64)
    (local $digit i32)
    (local $i i32)
    (local $neg i32)
    (local $intstr i32)
    (local $buf i32)
    (local $res i32)
    i32.const 0
    local.set $neg
    local.get $v
    f64.const 0
    f64.lt
    (if (then
        i32.const 1
        local.set $neg
        local.get $v
        f64.neg
        local.set $v
    ))
    local.get $v
    i32.trunc_f64_s
    local.set $ip
    local.get $v
    local.get $ip
    f64.convert_i32_s
    f64.sub
    local.set $frac
    local.get $ip
    call $int_to_string
    local.set $intstr
    i32.const 16
    i32.const 5
    call $malloc
    local.set $buf
    local.get $buf
    i32.const 46
    i32.store8
    i32.const 1
    local.set $i
    (block $fdone
        (loop $fgen
            local.get $i
            i32.const 7
            i32.ge_s
            br_if $fdone
            local.get $frac
            f64.const 10
            f64.mul
            local.set $frac
            local.get $frac
            i32.trunc_f64_s
            local.set $digit
            local.get $buf
            local.get $i
            i32.add
            local.get $digit
            i32.const 48
            i32.add
            i32.store8
            local.get $frac
            local.get $digit
            f64.convert_i32_s
            f64.sub
            local.set $frac
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $fgen
        )
    )
    local.get $buf
    local.get $i
    i32.add
    i32.const 0
    i32.store8
    local.get $intstr
    local.get $buf
    call $concat_strings
    local.set $res
    local.get $neg
    (if (then
        i32.const {minus}
        local.get $res
        call $concat_strings
        local.set $res
    ))
    local.get $res
)
(func $float_to_string (param $v f32) (result i32)
    local.get $v
    f64.promote_f32
    call $double_to_string
)
"#, minus = minus));
    }

    /// Emits a conversion turning a value of `type_name` already on the stack into a string
    /// pointer on the stack (used by struct/array defaults).
    fn emit_value_to_string(type_name: &str, writer: &mut IndentedTextWriter) {
        match strip_nullable(type_name) {
            "int" => writer.write_line("call $int_to_string"),
            "bool" => writer.write_line("call $bool_to_string"),
            "char" => writer.write_line("call $char_to_string"),
            "float" => writer.write_line("call $float_to_string"),
            "double" => writer.write_line("call $double_to_string"),
            "string" => {} // identity
            _ => writer.write_line("call $object_to_string"),
        }
    }

    /// Emits a conversion turning a value of `type_name` already on the stack into its hash
    /// (i32) on the stack.
    fn emit_value_to_hash(type_name: &str, writer: &mut IndentedTextWriter) {
        match strip_nullable(type_name) {
            "int" | "bool" | "char" => {}
            "float" => writer.write_line("i32.reinterpret_f32"),
            "double" => {
                writer.write_line("f32.demote_f64");
                writer.write_line("i32.reinterpret_f32");
            }
            "string" => writer.write_line("call $hash_string"),
            _ => writer.write_line("call $object_hash_code"),
        }
    }

    fn build_struct_protocol_defaults(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let infos: Vec<StructInfo> = self.struct_table.structs.values().cloned().collect();
        for info in &infos {
            if !self.has_protocol_override(&info.name, "to_string") {
                self.build_default_struct_to_string(info, writer)?;
            }
            if !self.has_protocol_override(&info.name, "hash_code") {
                self.build_default_struct_hash_code(info, writer)?;
            }
        }
        Ok(())
    }

    fn build_default_struct_to_string(&self, info: &StructInfo, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let (prefix, labels, suffix) = Self::struct_to_string_pieces(info);
        let fields = Self::sorted_fields(info);

        writer.write_line(&format!("(func ${}_to_string (param $this i32) (result i32)", info.name));
        writer.indent();
        writer.write_line("(local $res i32)");
        writer.write_line(&format!("i32.const {}", self.rstr(&prefix)));
        writer.write_line("local.set $res");

        for (idx, (_, field)) in fields.iter().enumerate() {
            let field_type = field.type_.get_type();
            // res = concat(res, label)
            writer.write_line("local.get $res");
            writer.write_line(&format!("i32.const {}", self.rstr(&labels[idx])));
            writer.write_line("call $concat_strings");
            writer.write_line("local.set $res");
            // res = concat(res, to_string(field))
            writer.write_line("local.get $res");
            writer.write_line("local.get $this");
            if field.offset > 0 {
                writer.write_line(&format!("i32.const {}", field.offset));
                writer.write_line("i32.add");
            }
            WasmGenerator::emit_load(&field_type, writer)?;
            Self::emit_value_to_string(&field_type, writer);
            writer.write_line("call $concat_strings");
            writer.write_line("local.set $res");
        }

        writer.write_line("local.get $res");
        writer.write_line(&format!("i32.const {}", self.rstr(&suffix)));
        writer.write_line("call $concat_strings");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    fn build_default_struct_hash_code(&self, info: &StructInfo, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let fields = Self::sorted_fields(info);
        writer.write_line(&format!("(func ${}_hash_code (param $this i32) (result i32)", info.name));
        writer.indent();
        writer.write_line("(local $h i32)");
        writer.write_line("i32.const 17");
        writer.write_line("local.set $h");
        for (_, field) in fields.iter() {
            let field_type = field.type_.get_type();
            writer.write_line("local.get $h");
            writer.write_line("i32.const 31");
            writer.write_line("i32.mul");
            writer.write_line("local.get $this");
            if field.offset > 0 {
                writer.write_line(&format!("i32.const {}", field.offset));
                writer.write_line("i32.add");
            }
            WasmGenerator::emit_load(&field_type, writer)?;
            Self::emit_value_to_hash(&field_type, writer);
            writer.write_line("i32.add");
            writer.write_line("local.set $h");
        }
        writer.write_line("local.get $h");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Returns the element types for which array helpers are generated: the primitives plus
    /// every known struct.
    fn array_element_types(&self) -> Vec<String> {
        let mut v: Vec<String> = PRIMITIVE_ARRAY_ELEMENTS.iter().map(|s| s.to_string()).collect();
        v.extend(self.sorted_struct_names());
        v
    }

    fn build_array_protocol_helpers(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let open = self.rstr("[");
        let close = self.rstr("]");
        let sep = self.rstr(", ");
        for elem in self.array_element_types() {
            let size = WasmGenerator::element_size_of(&elem);
            // to_string
            writer.write_line(&format!("(func $array_to_string_{} (param $ptr i32) (result i32)", elem));
            writer.indent();
            writer.write_line("(local $res i32)");
            writer.write_line("(local $len i32)");
            writer.write_line("(local $i i32)");
            writer.write_line(&format!("i32.const {}", open));
            writer.write_line("local.set $res");
            writer.write_line("local.get $ptr");
            writer.write_line("i32.load");
            writer.write_line("local.set $len");
            writer.write_line("i32.const 0");
            writer.write_line("local.set $i");
            writer.write_line("(block $done");
            writer.indent();
            writer.write_line("(loop $scan");
            writer.indent();
            writer.write_line("local.get $i");
            writer.write_line("local.get $len");
            writer.write_line("i32.ge_s");
            writer.write_line("br_if $done");
            writer.write_line("local.get $i");
            writer.write_line("i32.const 0");
            writer.write_line("i32.gt_s");
            writer.write_line("(if (then");
            writer.indent();
            writer.write_line("local.get $res");
            writer.write_line(&format!("i32.const {}", sep));
            writer.write_line("call $concat_strings");
            writer.write_line("local.set $res");
            writer.unindent();
            writer.write_line("))");
            writer.write_line("local.get $res");
            writer.write_line("local.get $ptr");
            writer.write_line("i32.const 4");
            writer.write_line("i32.add");
            writer.write_line("local.get $i");
            if size != 1 {
                writer.write_line(&format!("i32.const {}", size));
                writer.write_line("i32.mul");
            }
            writer.write_line("i32.add");
            WasmGenerator::emit_load(&elem, writer)?;
            Self::emit_value_to_string(&elem, writer);
            writer.write_line("call $concat_strings");
            writer.write_line("local.set $res");
            writer.write_line("local.get $i");
            writer.write_line("i32.const 1");
            writer.write_line("i32.add");
            writer.write_line("local.set $i");
            writer.write_line("br $scan");
            writer.unindent();
            writer.write_line(")");
            writer.unindent();
            writer.write_line(")");
            writer.write_line("local.get $res");
            writer.write_line(&format!("i32.const {}", close));
            writer.write_line("call $concat_strings");
            writer.unindent();
            writer.write_line(")");

            // hash_code
            writer.write_line(&format!("(func $array_hash_code_{} (param $ptr i32) (result i32)", elem));
            writer.indent();
            writer.write_line("(local $h i32)");
            writer.write_line("(local $len i32)");
            writer.write_line("(local $i i32)");
            writer.write_line("i32.const 17");
            writer.write_line("local.set $h");
            writer.write_line("local.get $ptr");
            writer.write_line("i32.load");
            writer.write_line("local.set $len");
            writer.write_line("i32.const 0");
            writer.write_line("local.set $i");
            writer.write_line("(block $done");
            writer.indent();
            writer.write_line("(loop $scan");
            writer.indent();
            writer.write_line("local.get $i");
            writer.write_line("local.get $len");
            writer.write_line("i32.ge_s");
            writer.write_line("br_if $done");
            writer.write_line("local.get $h");
            writer.write_line("i32.const 31");
            writer.write_line("i32.mul");
            writer.write_line("local.get $ptr");
            writer.write_line("i32.const 4");
            writer.write_line("i32.add");
            writer.write_line("local.get $i");
            if size != 1 {
                writer.write_line(&format!("i32.const {}", size));
                writer.write_line("i32.mul");
            }
            writer.write_line("i32.add");
            WasmGenerator::emit_load(&elem, writer)?;
            Self::emit_value_to_hash(&elem, writer);
            writer.write_line("i32.add");
            writer.write_line("local.set $h");
            writer.write_line("local.get $i");
            writer.write_line("i32.const 1");
            writer.write_line("i32.add");
            writer.write_line("local.set $i");
            writer.write_line("br $scan");
            writer.unindent();
            writer.write_line(")");
            writer.unindent();
            writer.write_line(")");
            writer.write_line("local.get $h");
            writer.unindent();
            writer.write_line(")");
        }
        Ok(())
    }

    /// Writes a `if (tag == n) { <body>; return }` dispatch arm.
    fn write_tag_arm(&self, tag: i32, body: &[&str], writer: &mut IndentedTextWriter) {
        writer.write_line("local.get $tag");
        writer.write_line(&format!("i32.const {}", tag));
        writer.write_line("i32.eq");
        writer.write_line("(if (then");
        writer.indent();
        for line in body {
            writer.write_line(line);
        }
        writer.write_line("return");
        writer.unindent();
        writer.write_line("))");
    }

    fn build_object_to_string(&self, writer: &mut IndentedTextWriter) {
        let null = self.rstr("null");
        let fallback = self.rstr("<object>");
        writer.write_line("(func $object_to_string (param $ptr i32) (result i32)");
        writer.indent();
        writer.write_line("(local $tag i32)");
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("(if (then");
        writer.indent();
        writer.write_line(&format!("i32.const {}", null));
        writer.write_line("return");
        writer.unindent();
        writer.write_line("))");
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_tag");
        writer.write_line("local.set $tag");
        self.write_tag_arm(TAG_INT, &["local.get $ptr", "call $unbox_int", "call $int_to_string"], writer);
        self.write_tag_arm(TAG_FLOAT, &["local.get $ptr", "call $unbox_float", "call $float_to_string"], writer);
        self.write_tag_arm(TAG_DOUBLE, &["local.get $ptr", "call $unbox_double", "call $double_to_string"], writer);
        self.write_tag_arm(TAG_BOOL, &["local.get $ptr", "call $unbox_bool", "call $bool_to_string"], writer);
        self.write_tag_arm(TAG_CHAR, &["local.get $ptr", "call $unbox_char", "call $char_to_string"], writer);
        self.write_tag_arm(TAG_STRING, &["local.get $ptr"], writer);
        for (i, name) in self.sorted_struct_names().iter().enumerate() {
            let call = format!("call ${}_to_string", name);
            self.write_tag_arm(TAG_STRUCT_BASE + i as i32, &["local.get $ptr", &call], writer);
        }
        writer.write_line(&format!("i32.const {}", fallback));
        writer.unindent();
        writer.write_line(")");
    }

    fn build_object_hash_code(&self, writer: &mut IndentedTextWriter) {
        writer.write_line("(func $object_hash_code (param $ptr i32) (result i32)");
        writer.indent();
        writer.write_line("(local $tag i32)");
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("(if (then");
        writer.indent();
        writer.write_line("i32.const 0");
        writer.write_line("return");
        writer.unindent();
        writer.write_line("))");
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_tag");
        writer.write_line("local.set $tag");
        self.write_tag_arm(TAG_INT, &["local.get $ptr", "call $unbox_int"], writer);
        self.write_tag_arm(TAG_FLOAT, &["local.get $ptr", "call $unbox_float", "call $hash_float"], writer);
        self.write_tag_arm(TAG_DOUBLE, &["local.get $ptr", "call $unbox_double", "call $hash_double"], writer);
        self.write_tag_arm(TAG_BOOL, &["local.get $ptr", "call $unbox_bool"], writer);
        self.write_tag_arm(TAG_CHAR, &["local.get $ptr", "call $unbox_char"], writer);
        self.write_tag_arm(TAG_STRING, &["local.get $ptr", "call $hash_string"], writer);
        for (i, name) in self.sorted_struct_names().iter().enumerate() {
            let call = format!("call ${}_hash_code", name);
            self.write_tag_arm(TAG_STRUCT_BASE + i as i32, &["local.get $ptr", &call], writer);
        }
        // Fallback: pointer identity.
        writer.write_line("local.get $ptr");
        writer.unindent();
        writer.write_line(")");
    }

    fn build_print_object(&self, writer: &mut IndentedTextWriter) {
        let null = self.rstr("null");
        writer.write_line("(func $print_object (param $ptr i32)");
        writer.indent();
        writer.write_line("(local $tag i32)");
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("(if (then");
        writer.indent();
        writer.write_line(&format!("i32.const {}", null));
        writer.write_line("call $print_string");
        writer.write_line("return");
        writer.unindent();
        writer.write_line("))");
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_tag");
        writer.write_line("local.set $tag");
        self.write_tag_arm(TAG_INT, &["local.get $ptr", "call $unbox_int", "call $print_int"], writer);
        self.write_tag_arm(TAG_FLOAT, &["local.get $ptr", "call $unbox_float", "call $print_float"], writer);
        self.write_tag_arm(TAG_DOUBLE, &["local.get $ptr", "call $unbox_double", "call $print_double"], writer);
        self.write_tag_arm(TAG_BOOL, &["local.get $ptr", "call $unbox_bool", "call $bool_to_string", "call $print_string"], writer);
        self.write_tag_arm(TAG_CHAR, &["local.get $ptr", "call $unbox_char", "call $print_char"], writer);
        self.write_tag_arm(TAG_STRING, &["local.get $ptr", "call $print_string"], writer);
        // Structs and arrays: render via to_string then print.
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_to_string");
        writer.write_line("call $print_string");
        writer.unindent();
        writer.write_line(")");
    }

    fn build_release_object(&self, writer: &mut IndentedTextWriter) {
        writer.write_line("(func $release_object (param $ptr i32)");
        writer.indent();
        writer.write_line("(local $tag i32)");
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("br_if 0");
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_tag");
        writer.write_line("local.set $tag");
        self.write_tag_arm(TAG_STRING, &["local.get $ptr", "call $release_string"], writer);
        for (i, name) in self.sorted_struct_names().iter().enumerate() {
            let call = format!("call $release_{}", name);
            self.write_tag_arm(TAG_STRUCT_BASE + i as i32, &["local.get $ptr", &call], writer);
        }
        // Boxed primitives, arrays, and unknown tags: drop one reference, free at zero.
        writer.write_line("local.get $ptr");
        writer.write_line("call $release_generic");
        writer.unindent();
        writer.write_line(")");
    }

    // ----- Expression-level wiring for the builtins -----

    /// Builds `to_string(arg)` leaving a string pointer on the stack.
    pub fn build_to_string(&mut self, arg: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let t = self.infer_expression_type(arg, function)?;
        // Enum values are plain i32s at runtime; render them like ints.
        let base = self.enum_or_int(strip_nullable(&t));
        if base.ends_with("[]") {
            let elem = base[..base.len() - 2].to_string();
            if self.array_element_types().contains(&elem) {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line(&format!("call $array_to_string_{}", elem));
                return Ok(());
            }
        }
        self.build_expression(arg, &t, function, writer)?;
        match base.as_str() {
            "int" => writer.write_line("call $int_to_string"),
            "bool" => writer.write_line("call $bool_to_string"),
            "char" => writer.write_line("call $char_to_string"),
            "float" => writer.write_line("call $float_to_string"),
            "double" => writer.write_line("call $double_to_string"),
            "string" => {}
            _ => writer.write_line("call $object_to_string"),
        }
        Ok(())
    }

    /// Builds `hash_code(arg)` leaving an i32 on the stack.
    pub fn build_hash_code(&mut self, arg: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let t = self.infer_expression_type(arg, function)?;
        let base = self.enum_or_int(strip_nullable(&t));
        if base.ends_with("[]") {
            let elem = base[..base.len() - 2].to_string();
            if self.array_element_types().contains(&elem) {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line(&format!("call $array_hash_code_{}", elem));
                return Ok(());
            }
        }
        self.build_expression(arg, &t, function, writer)?;
        match base.as_str() {
            "int" | "bool" | "char" => {}
            "float" => writer.write_line("i32.reinterpret_f32"),
            "double" => {
                writer.write_line("f32.demote_f64");
                writer.write_line("i32.reinterpret_f32");
            }
            "string" => writer.write_line("call $hash_string"),
            _ => writer.write_line("call $object_hash_code"),
        }
        Ok(())
    }

    /// Builds `print(arg)`. Primitives go straight to the matching host `print_*` (so numeric
    /// values keep their trailing newline); objects dispatch at runtime; other reference types
    /// render via `to_string`.
    pub fn build_print(&mut self, arg: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let t = self.infer_expression_type(arg, function)?;
        let base = self.enum_or_int(strip_nullable(&t));
        match base.as_str() {
            "int" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_int");
            }
            "float" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_float");
            }
            "double" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_double");
            }
            "bool" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $bool_to_string");
                writer.write_line("call $print_string");
            }
            "char" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_char");
            }
            "string" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_string");
            }
            "object" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_object");
            }
            _ => {
                self.build_to_string(arg, function, writer)?;
                writer.write_line("call $print_string");
            }
        }
        Ok(())
    }

    /// Builds `println(arg)`: prints the value (no trailing newline from `print`) followed by a
    /// single `\n` (code point 10) via the char host.
    pub fn build_println(&mut self, arg: &ExpressionNode<'a>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        self.build_print(arg, function, writer)?;
        writer.write_line("i32.const 10");
        writer.write_line("call $print_char");
        Ok(())
    }
}
