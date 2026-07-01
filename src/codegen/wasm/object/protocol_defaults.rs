//! Per-type *generated* object-protocol defaults: the `to_string`/`hash_code` bodies synthesized
//! for each struct, discriminated union, and primitive-array element type, plus the tag-dispatching
//! `$object_to_string`/`$object_hash_code`/`$print_object`/`$release_object` routers. Split out of
//! the former `object.rs` god module; the fixed runtime and expression wiring live in the sibling
//! `mod` / `expressions` modules.

use super::{
    PRIMITIVE_ARRAY_ELEMENTS, TAG_BOOL, TAG_BYTE, TAG_CHAR, TAG_DOUBLE, TAG_FLOAT, TAG_INT,
    TAG_LONG, TAG_STRING, TAG_STRUCT_BASE, TAG_UINT, TAG_ULONG,
};
use crate::codegen::wasm::WasmGenerator;
use crate::semantics::struct_table::StructInfo;
use crate::syntax::nodes::types::strip_nullable;
use crate::text::indented_text_writer::IndentedTextWriter;
use crate::codegen::CodegenError as Error;

impl<'a> WasmGenerator<'a> {
    /// Emits a conversion turning a value of `type_name` already on the stack into a string
    /// pointer on the stack (used by struct/array defaults).
    fn emit_value_to_string(type_name: &str, writer: &mut IndentedTextWriter) {
        match strip_nullable(type_name) {
            "int" => writer.write_line("call $int_to_string"),
            "bool" => writer.write_line("call $bool_to_string"),
            "char" => writer.write_line("call $char_to_string"),
            "float" => writer.write_line("call $float_to_string"),
            "double" => writer.write_line("call $double_to_string"),
            "long" => writer.write_line("call $long_to_string"),
            "ulong" => writer.write_line("call $ulong_to_string"),
            "uint" => writer.write_line("call $uint_to_string"),
            "byte" => writer.write_line("call $byte_to_string"),
            "string" => {} // identity
            _ => writer.write_line("call $object_to_string"),
        }
    }

    /// Emits a conversion turning a value of `type_name` already on the stack into its hash
    /// (i32) on the stack.
    fn emit_value_to_hash(type_name: &str, writer: &mut IndentedTextWriter) {
        match strip_nullable(type_name) {
            "int" | "bool" | "char" | "uint" | "byte" => {}
            "long" | "ulong" => writer.write_line("call $hash_long"),
            "float" => writer.write_line("i32.reinterpret_f32"),
            "double" => {
                writer.write_line("f32.demote_f64");
                writer.write_line("i32.reinterpret_f32");
            }
            "string" => writer.write_line("call $hash_string"),
            _ => writer.write_line("call $object_hash_code"),
        }
    }

    pub(super) fn build_struct_protocol_defaults(
        &self,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let infos: Vec<StructInfo> = self.struct_table.structs.values().cloned().collect();
        for info in &infos {
            // Discriminated unions are registered in the struct table too (for tagging/release),
            // but their payload lives in the union table. Use variant-aware defaults that read the
            // discriminant and active payload instead of the (empty) struct field map.
            if let Some(union_info) = self.unions.get(&info.name) {
                if !self.has_protocol_override(&info.name, crate::intrinsics::TO_STRING) {
                    self.build_default_union_to_string(union_info, writer)?;
                }
                if !self.has_protocol_override(&info.name, crate::intrinsics::HASH_CODE) {
                    self.build_default_union_hash_code(union_info, writer)?;
                }
                continue;
            }
            if !self.has_protocol_override(&info.name, crate::intrinsics::TO_STRING) {
                self.build_default_struct_to_string(info, writer)?;
            }
            if !self.has_protocol_override(&info.name, crate::intrinsics::HASH_CODE) {
                self.build_default_struct_hash_code(info, writer)?;
            }
        }
        Ok(())
    }

    /// The `(prefix, field-labels, suffix)` literal pieces for one union variant's `to_string`.
    /// Data variants render as `Variant(a: <a>, b: <b>)`; unit variants render as just `Variant`.
    pub(super) fn union_variant_to_string_pieces(
        variant: &crate::semantics::union_table::UnionVariantInfo,
    ) -> (String, Vec<String>, String) {
        if variant.fields.is_empty() {
            return (variant.name.clone(), Vec::new(), String::new());
        }
        let prefix = format!("{}(", variant.name);
        let labels = variant
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if i == 0 {
                    format!("{}: ", f.name)
                } else {
                    format!(", {}: ", f.name)
                }
            })
            .collect();
        (prefix, labels, ")".to_string())
    }

    fn build_default_union_to_string(
        &self,
        union_info: &crate::semantics::union_table::UnionInfo,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        writer.write_line(&format!(
            "(func ${}_to_string (param $this i32) (result i32)",
            union_info.name
        ));
        writer.indent();
        writer.write_line("(local $res i32)");
        writer.write_line("(local $d i32)");
        // Default to "<object>" so an out-of-range discriminant still yields a valid string.
        writer.write_line(&format!("i32.const {}", self.rstr("<object>")));
        writer.write_line("local.set $res");
        writer.write_line("local.get $this");
        writer.write_line("i32.load");
        writer.write_line("local.set $d");

        for variant in &union_info.variants {
            let (prefix, labels, suffix) = Self::union_variant_to_string_pieces(variant);
            writer.write_line("local.get $d");
            writer.write_line(&format!("i32.const {}", variant.discriminant));
            writer.write_line("i32.eq");
            writer.write_line("if");
            writer.indent();
            writer.write_line(&format!("i32.const {}", self.rstr(&prefix)));
            writer.write_line("local.set $res");
            for (idx, field) in variant.fields.iter().enumerate() {
                let field_type = field.type_.get_type();
                // res = concat(res, label)
                writer.write_line("local.get $res");
                writer.write_line(&format!("i32.const {}", self.rstr(&labels[idx])));
                writer.write_line("call $concat_strings");
                writer.write_line("local.set $res");
                // res = concat(res, to_string(field))
                writer.write_line("local.get $res");
                writer.write_line("local.get $this");
                writer.write_line(&format!("i32.const {}", field.offset));
                writer.write_line("i32.add");
                WasmGenerator::emit_load(&field_type, writer)?;
                Self::emit_value_to_string(&field_type, writer);
                writer.write_line("call $concat_strings");
                writer.write_line("local.set $res");
            }
            // res = concat(res, suffix)
            writer.write_line("local.get $res");
            writer.write_line(&format!("i32.const {}", self.rstr(&suffix)));
            writer.write_line("call $concat_strings");
            writer.write_line("local.set $res");
            writer.unindent();
            writer.write_line("end");
        }

        writer.write_line("local.get $res");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    fn build_default_union_hash_code(
        &self,
        union_info: &crate::semantics::union_table::UnionInfo,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        writer.write_line(&format!(
            "(func ${}_hash_code (param $this i32) (result i32)",
            union_info.name
        ));
        writer.indent();
        writer.write_line("(local $h i32)");
        writer.write_line("(local $d i32)");
        writer.write_line("local.get $this");
        writer.write_line("i32.load");
        writer.write_line("local.set $d");
        // Seed with the discriminant so distinct variants hash differently even with no payload.
        writer.write_line("local.get $d");
        writer.write_line("local.set $h");

        for variant in &union_info.variants {
            if variant.fields.is_empty() {
                continue;
            }
            writer.write_line("local.get $d");
            writer.write_line(&format!("i32.const {}", variant.discriminant));
            writer.write_line("i32.eq");
            writer.write_line("if");
            writer.indent();
            for field in &variant.fields {
                let field_type = field.type_.get_type();
                writer.write_line("local.get $h");
                writer.write_line("i32.const 31");
                writer.write_line("i32.mul");
                writer.write_line("local.get $this");
                writer.write_line(&format!("i32.const {}", field.offset));
                writer.write_line("i32.add");
                WasmGenerator::emit_load(&field_type, writer)?;
                Self::emit_value_to_hash(&field_type, writer);
                writer.write_line("i32.add");
                writer.write_line("local.set $h");
            }
            writer.unindent();
            writer.write_line("end");
        }

        writer.write_line("local.get $h");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    fn build_default_struct_to_string(
        &self,
        info: &StructInfo,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let (prefix, labels, suffix) = Self::struct_to_string_pieces(info);
        let fields = Self::sorted_fields(info);

        writer.write_line(&format!(
            "(func ${}_to_string (param $this i32) (result i32)",
            info.name
        ));
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

    fn build_default_struct_hash_code(
        &self,
        info: &StructInfo,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let fields = Self::sorted_fields(info);
        writer.write_line(&format!(
            "(func ${}_hash_code (param $this i32) (result i32)",
            info.name
        ));
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
    pub(crate) fn array_element_types(&self) -> Vec<String> {
        let mut v: Vec<String> = PRIMITIVE_ARRAY_ELEMENTS
            .iter()
            .map(|s| s.to_string())
            .collect();
        v.extend(self.sorted_struct_names());
        v
    }

    pub(super) fn build_array_protocol_helpers(
        &self,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let open = self.rstr("[");
        let close = self.rstr("]");
        let sep = self.rstr(", ");
        for elem in self.array_element_types() {
            let size = WasmGenerator::element_size_of(&elem);
            // to_string
            writer.write_line(&format!(
                "(func $array_to_string_{} (param $ptr i32) (result i32)",
                elem
            ));
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
            writer.write_line(&format!(
                "(func $array_hash_code_{} (param $ptr i32) (result i32)",
                elem
            ));
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

    pub(super) fn build_object_to_string(&self, writer: &mut IndentedTextWriter) {
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
        self.write_tag_arm(
            TAG_INT,
            &["local.get $ptr", "call $unbox_int", "call $int_to_string"],
            writer,
        );
        self.write_tag_arm(
            TAG_FLOAT,
            &[
                "local.get $ptr",
                "call $unbox_float",
                "call $float_to_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_DOUBLE,
            &[
                "local.get $ptr",
                "call $unbox_double",
                "call $double_to_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_BOOL,
            &["local.get $ptr", "call $unbox_bool", "call $bool_to_string"],
            writer,
        );
        self.write_tag_arm(
            TAG_CHAR,
            &["local.get $ptr", "call $unbox_char", "call $char_to_string"],
            writer,
        );
        self.write_tag_arm(
            TAG_LONG,
            &["local.get $ptr", "call $unbox_long", "call $long_to_string"],
            writer,
        );
        self.write_tag_arm(
            TAG_ULONG,
            &[
                "local.get $ptr",
                "call $unbox_ulong",
                "call $ulong_to_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_UINT,
            &["local.get $ptr", "call $unbox_uint", "call $uint_to_string"],
            writer,
        );
        self.write_tag_arm(
            TAG_BYTE,
            &["local.get $ptr", "call $unbox_byte", "call $byte_to_string"],
            writer,
        );
        self.write_tag_arm(TAG_STRING, &["local.get $ptr"], writer);
        for (i, name) in self.sorted_struct_names().iter().enumerate() {
            let call = format!("call ${}_to_string", name);
            self.write_tag_arm(
                TAG_STRUCT_BASE + i as i32,
                &["local.get $ptr", &call],
                writer,
            );
        }
        writer.write_line(&format!("i32.const {}", fallback));
        writer.unindent();
        writer.write_line(")");
    }

    pub(super) fn build_object_hash_code(&self, writer: &mut IndentedTextWriter) {
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
        self.write_tag_arm(
            TAG_FLOAT,
            &["local.get $ptr", "call $unbox_float", "call $hash_float"],
            writer,
        );
        self.write_tag_arm(
            TAG_DOUBLE,
            &["local.get $ptr", "call $unbox_double", "call $hash_double"],
            writer,
        );
        self.write_tag_arm(TAG_BOOL, &["local.get $ptr", "call $unbox_bool"], writer);
        self.write_tag_arm(TAG_CHAR, &["local.get $ptr", "call $unbox_char"], writer);
        self.write_tag_arm(
            TAG_LONG,
            &["local.get $ptr", "call $unbox_long", "call $hash_long"],
            writer,
        );
        self.write_tag_arm(
            TAG_ULONG,
            &["local.get $ptr", "call $unbox_ulong", "call $hash_long"],
            writer,
        );
        self.write_tag_arm(TAG_UINT, &["local.get $ptr", "call $unbox_uint"], writer);
        self.write_tag_arm(TAG_BYTE, &["local.get $ptr", "call $unbox_byte"], writer);
        self.write_tag_arm(TAG_STRING, &["local.get $ptr", "call $hash_string"], writer);
        for (i, name) in self.sorted_struct_names().iter().enumerate() {
            let call = format!("call ${}_hash_code", name);
            self.write_tag_arm(
                TAG_STRUCT_BASE + i as i32,
                &["local.get $ptr", &call],
                writer,
            );
        }
        // Fallback: pointer identity.
        writer.write_line("local.get $ptr");
        writer.unindent();
        writer.write_line(")");
    }

    pub(super) fn build_print_object(&self, writer: &mut IndentedTextWriter) {
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
        self.write_tag_arm(
            TAG_INT,
            &["local.get $ptr", "call $unbox_int", "call $print_int"],
            writer,
        );
        self.write_tag_arm(
            TAG_FLOAT,
            &["local.get $ptr", "call $unbox_float", "call $print_float"],
            writer,
        );
        self.write_tag_arm(
            TAG_DOUBLE,
            &["local.get $ptr", "call $unbox_double", "call $print_double"],
            writer,
        );
        self.write_tag_arm(
            TAG_BOOL,
            &[
                "local.get $ptr",
                "call $unbox_bool",
                "call $bool_to_string",
                "call $print_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_CHAR,
            &["local.get $ptr", "call $unbox_char", "call $print_char"],
            writer,
        );
        self.write_tag_arm(
            TAG_LONG,
            &[
                "local.get $ptr",
                "call $unbox_long",
                "call $long_to_string",
                "call $print_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_ULONG,
            &[
                "local.get $ptr",
                "call $unbox_ulong",
                "call $ulong_to_string",
                "call $print_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_UINT,
            &[
                "local.get $ptr",
                "call $unbox_uint",
                "call $uint_to_string",
                "call $print_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_BYTE,
            &[
                "local.get $ptr",
                "call $unbox_byte",
                "call $byte_to_string",
                "call $print_string",
            ],
            writer,
        );
        self.write_tag_arm(
            TAG_STRING,
            &["local.get $ptr", "call $print_string"],
            writer,
        );
        // Structs and arrays: render via to_string then print.
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_to_string");
        writer.write_line("call $print_string");
        writer.unindent();
        writer.write_line(")");
    }

    pub(super) fn build_release_object(&self, writer: &mut IndentedTextWriter) {
        writer.write_line("(func $release_object (param $ptr i32)");
        writer.indent();
        writer.write_line("(local $tag i32)");
        writer.write_line("local.get $ptr");
        writer.write_line("i32.eqz");
        writer.write_line("br_if 0");
        writer.write_line("local.get $ptr");
        writer.write_line("call $object_tag");
        writer.write_line("local.set $tag");
        self.write_tag_arm(
            TAG_STRING,
            &["local.get $ptr", "call $release_string"],
            writer,
        );
        for (i, name) in self.sorted_struct_names().iter().enumerate() {
            let call = format!("call $release_{}", name);
            self.write_tag_arm(
                TAG_STRUCT_BASE + i as i32,
                &["local.get $ptr", &call],
                writer,
            );
        }
        // Boxed primitives, arrays, and unknown tags: drop one reference, free at zero.
        writer.write_line("local.get $ptr");
        writer.write_line("call $release_generic");
        writer.unindent();
        writer.write_line(")");
    }
}
