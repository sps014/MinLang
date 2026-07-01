use super::WasmGenerator;
use crate::semantics::struct_table::StructInfo;
use crate::syntax::nodes::types::{
    is_boxable_primitive, method_fn, strip_nullable, PRIMITIVE_TYPE_NAMES,
};
use crate::text::indented_text_writer::IndentedTextWriter;
use crate::codegen::CodegenError as Error;

/// Runtime type tags stored in each heap block's header. Reference types carry their tag in
/// the block they already own; primitives are boxed into a small tagged block.
pub const TAG_INT: i32 = 1;
pub const TAG_FLOAT: i32 = 2;
pub const TAG_DOUBLE: i32 = 3;
pub const TAG_BOOL: i32 = 4;
pub const TAG_STRING: i32 = 5;
pub const TAG_ARRAY: i32 = 6;
pub const TAG_CHAR: i32 = 7;
pub const TAG_LONG: i32 = 8;
pub const TAG_UINT: i32 = 9;
pub const TAG_ULONG: i32 = 10;
pub const TAG_BYTE: i32 = 11;
/// Structs are assigned consecutive tags starting here, ordered by sorted struct name.
pub const TAG_STRUCT_BASE: i32 = 12;

/// Element types for which array `to_string`/`hash_code` helpers are generated.
const PRIMITIVE_ARRAY_ELEMENTS: [&str; PRIMITIVE_TYPE_NAMES.len()] = PRIMITIVE_TYPE_NAMES;

/// The fixed object-protocol runtime that does not depend on the user program: boxing /
/// unboxing of primitives, primitive hashers, and `$int_to_string` (digit extraction). Its
/// `{TAG_*}` placeholders are substituted from the `TAG_*` constants so the `.wat` never
/// hardcodes a raw tag literal (see [`WasmGenerator::object_runtime_wat`]).
const OBJECT_RUNTIME_FIXED: &str = include_str!("../runtime/object.wat");

/// The program-independent `double`/`float` decimal formatter. `{minus}` (the `"-"` string-table
/// offset) and `{TAG_STRING}` are substituted in (see [`WasmGenerator::build_float_double_to_string`]).
const RUNTIME_FORMAT: &str = include_str!("../runtime/format.wat");

impl<'a> WasmGenerator<'a> {
    /// Returns true for the boxable scalar primitives.
    pub fn is_primitive_name(name: &str) -> bool {
        is_boxable_primitive(name)
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
            "long" => TAG_LONG,
            "uint" => TAG_UINT,
            "ulong" => TAG_ULONG,
            "byte" => TAG_BYTE,
            "object" => 0,
            _ => match self.sorted_struct_names().iter().position(|n| n == base) {
                Some(i) => TAG_STRUCT_BASE + i as i32,
                None => 0,
            },
        }
    }

    /// Fields of a struct ordered by their byte offset (i.e. declaration order).
    pub(crate) fn sorted_fields(
        info: &StructInfo,
    ) -> Vec<(String, &crate::semantics::struct_table::StructFieldInfo)> {
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
        *self
            .ctx
            .runtime_strings
            .get(content)
            .unwrap_or_else(|| panic!("runtime string not interned: {:?}", content))
    }

    /// The `(prefix, field-labels, suffix)` literal pieces used by a struct's default
    /// `to_string`. Field labels are in declaration (offset) order.
    fn struct_to_string_pieces(info: &StructInfo) -> (String, Vec<String>, String) {
        let prefix = format!("{} {{ ", info.name);
        let labels = Self::sorted_fields(info)
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                if i == 0 {
                    format!("{}: ", name)
                } else {
                    format!(", {}: ", name)
                }
            })
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
        // Discriminated-union default `to_string` pieces (per variant), since unions get
        // variant-aware defaults rather than the (empty) struct field rendering.
        let union_infos: Vec<crate::semantics::union_table::UnionInfo> =
            self.unions.values().cloned().collect();
        for union_info in &union_infos {
            for variant in &union_info.variants {
                let (prefix, labels, suffix) = Self::union_variant_to_string_pieces(variant);
                self.intern_runtime_string(&prefix);
                for label in &labels {
                    self.intern_runtime_string(label);
                }
                self.intern_runtime_string(&suffix);
            }
        }

        // Every enum variant name, returned by `EnumValue.name()`.
        let member_names: Vec<String> = self
            .enums
            .values()
            .flat_map(|members| members.keys().cloned())
            .collect();
        for name in member_names {
            self.intern_runtime_string(&name);
        }
    }

    /// True if the struct provides its own `@override` implementation of `method`
    /// (`to_string` / `hash_code`); otherwise a default is generated.
    fn has_protocol_override(&self, struct_name: &str, method: &str) -> bool {
        self.function_table
            .get_function(&method_fn(struct_name, method))
            .is_ok()
    }

    /// Emits one `$enum_name_<Enum>(i32) -> i32` lookup per declared enum, returning the
    /// interned variant-name string for a value (or the empty string if none matches, which
    /// only happens for out-of-range values produced by an `int` -> enum cast).
    pub fn build_enum_runtime(&self, writer: &mut IndentedTextWriter) {
        let fallback = self.rstr("");
        let mut enum_names: Vec<&String> = self.enums.keys().collect();
        enum_names.sort();
        for enum_name in enum_names {
            writer.write_line(&format!(
                "(func $enum_name_{} (param $v i32) (result i32)",
                enum_name
            ));
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

    /// Materializes [`OBJECT_RUNTIME_FIXED`] with the `{TAG_*}` placeholders substituted from the
    /// `TAG_*` constants, keeping the runtime tags as a single source of truth. Substituted values
    /// equal the previous raw literals, so emitted bytes are unchanged. Kept separate from emission
    /// so it is trivially unit-testable.
    fn object_runtime_wat() -> String {
        OBJECT_RUNTIME_FIXED
            .replace("{TAG_INT}", &TAG_INT.to_string())
            .replace("{TAG_FLOAT}", &TAG_FLOAT.to_string())
            .replace("{TAG_DOUBLE}", &TAG_DOUBLE.to_string())
            .replace("{TAG_BOOL}", &TAG_BOOL.to_string())
            .replace("{TAG_STRING}", &TAG_STRING.to_string())
            .replace("{TAG_CHAR}", &TAG_CHAR.to_string())
            .replace("{TAG_LONG}", &TAG_LONG.to_string())
            .replace("{TAG_UINT}", &TAG_UINT.to_string())
            .replace("{TAG_ULONG}", &TAG_ULONG.to_string())
            .replace("{TAG_BYTE}", &TAG_BYTE.to_string())
    }

    /// Emits the entire object-protocol runtime into the module.
    pub fn build_object_runtime(&self, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        writer.write_block(&Self::object_runtime_wat());
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

    /// Emits `$double_to_string`/`$float_to_string`. The body lives in `runtime/format.wat`;
    /// `double_to_string` rounds `|v|` to 6 decimal places using integer micro-units, then trims
    /// trailing zeros (so `2.5`, not `2.500000`, and `3`, not `3.000000`). Rounding via integers
    /// also avoids the floor-drift and float noise of digit-by-digit truncation (e.g. a result of
    /// `4.000000000000004` prints as `4`). Identical on every runtime since no host formatter is
    /// involved. `{minus}` (the `"-"` string-table offset) and `{TAG_STRING}` are program/tag
    /// dependent so they are substituted here rather than baked into the `.wat`.
    fn build_float_double_to_string(&self, writer: &mut IndentedTextWriter) {
        let minus = self.rstr("-");
        let runtime = RUNTIME_FORMAT
            .replace("{minus}", &minus.to_string())
            .replace("{TAG_STRING}", &TAG_STRING.to_string());
        writer.write_block(&runtime);
    }
}

mod expressions;
mod protocol_defaults;

#[cfg(test)]
mod object_runtime_tests {
    use super::{RUNTIME_FORMAT, TAG_STRING};
    use crate::codegen::wasm::WasmGenerator;

    /// Every `{TAG_*}` placeholder in `runtime/object.wat` must be substituted; if a new tag
    /// placeholder is added without a matching replacement, this catches it before codegen emits a
    /// literal `{` into the module.
    #[test]
    fn object_runtime_has_no_unsubstituted_placeholders() {
        let wat = WasmGenerator::object_runtime_wat();
        assert!(
            !wat.contains('{') && !wat.contains('}'),
            "object runtime still contains an unsubstituted placeholder"
        );
    }

    /// `runtime/format.wat` may only carry the two program/tag-dependent placeholders (`{minus}`
    /// and `{TAG_STRING}`); once both are substituted no brace may remain.
    #[test]
    fn format_runtime_has_no_unexpected_placeholders() {
        let wat = RUNTIME_FORMAT
            .replace("{minus}", "0")
            .replace("{TAG_STRING}", &TAG_STRING.to_string());
        assert!(
            !wat.contains('{') && !wat.contains('}'),
            "format runtime contains an unexpected placeholder beyond minus/TAG_STRING"
        );
    }
}
