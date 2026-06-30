//! Expression-level wiring for the object-protocol builtins (`to_string`, `hash_code`, `print`,
//! `println`): they infer the argument type and dispatch to the matching generated/runtime helper.
//! Split out of the former `object.rs` god module.

use crate::codegen::wasm::WasmGenerator;
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::nodes::{ExpressionNode, FunctionNode};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use std::io::Error;

impl<'a> WasmGenerator<'a> {
    // ----- Expression-level wiring for the builtins -----

    /// Builds `to_string(arg)` leaving a string pointer on the stack.
    pub fn build_to_string(
        &mut self,
        arg: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
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
            "long" => writer.write_line("call $long_to_string"),
            "ulong" => writer.write_line("call $ulong_to_string"),
            "uint" => writer.write_line("call $uint_to_string"),
            "byte" => writer.write_line("call $byte_to_string"),
            "string" => {}
            _ => writer.write_line("call $object_to_string"),
        }
        Ok(())
    }

    /// Builds `hash_code(arg)` leaving an i32 on the stack.
    pub fn build_hash_code(
        &mut self,
        arg: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
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
        Ok(())
    }

    /// Builds `print(arg)`. Primitives go straight to the matching host `print_*` (so numeric
    /// values keep their trailing newline); objects dispatch at runtime; other reference types
    /// render via `to_string`.
    pub fn build_print(
        &mut self,
        arg: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let t = self.infer_expression_type(arg, function)?;
        let base = self.enum_or_int(strip_nullable(&t));
        match base.as_str() {
            "int" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $print_int");
            }
            // Float/double print through the same in-wasm formatter as `to_string`, string
            // concatenation, and JSON, so the rendering is identical and deterministic across
            // runtimes (the host `print_double` exposes float noise like `4.000000000000004`).
            "float" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $float_to_string");
                writer.write_line("call $print_string");
            }
            "double" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line("call $double_to_string");
                writer.write_line("call $print_string");
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
            // The new integer types render through their in-wasm `*_to_string` (no host import)
            // then print, matching the float/double approach.
            "long" | "ulong" | "uint" | "byte" => {
                self.build_expression(arg, &t, function, writer)?;
                writer.write_line(&format!("call ${}_to_string", base));
                writer.write_line("call $print_string");
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
    pub fn build_println(
        &mut self,
        arg: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        self.build_print(arg, function, writer)?;
        writer.write_line("i32.const 10");
        writer.write_line("call $print_char");
        Ok(())
    }
}
