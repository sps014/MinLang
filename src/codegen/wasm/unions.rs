//! Code generation for discriminated unions: constructing variant values (a tagged heap block
//! holding the discriminant at offset 0 plus the active variant's payload) and lowering `match`
//! (a nested `if/else` over the discriminant with payload bindings, guards, and arm values).

use super::WasmGenerator;
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, MatchArm, MatchArmBody, PatternNode};
use crate::text::indented_text_writer::IndentedTextWriter;
use crate::codegen::CodegenError as Error;

/// The byte-offset chain (each step is "+offset, then load") that reaches the union pointer of the
/// value currently under a pattern. `leaf = Some(off)` means the value is field `off` of the
/// pointer reached by `parent_chain`; `None` means the value is the pointer/primitive itself.
/// Recursion-invariant inputs for [`WasmGenerator::build_match_arms`]. Only `index` and the
/// `writer` change as the nested `if/else` chain is emitted, so everything else is bundled here and
/// passed by shared reference.
struct MatchArmsCtx<'a, 'b> {
    subj_local: &'b str,
    base_type: &'b str,
    arms: &'b [MatchArm<'a>],
    value_type: Option<&'b str>,
    function: &'b FunctionNode<'a>,
}

fn cur_ptr_chain(parent_chain: &[usize], leaf: Option<usize>) -> Vec<usize> {
    match leaf {
        Some(off) => {
            let mut c = parent_chain.to_vec();
            c.push(off);
            c
        }
        None => parent_chain.to_vec(),
    }
}

impl<'a> WasmGenerator<'a> {
    /// Resolves a construction receiver (`Shape`, `Option`) and the expected type to the concrete
    /// monomorphized union name, or `None` if the receiver does not name a union. Non-generic
    /// unions resolve to their own name; generic unions take the concrete name from the expected
    /// type (e.g. `Option_int` for `let o: Option<int> = Option.Some(1)`).
    pub fn resolve_union_name(&self, receiver: &str, left_side: &str) -> Option<String> {
        if self.unions.contains_key(receiver) {
            return Some(receiver.to_string());
        }
        let base = strip_nullable(left_side);
        if self.unions.contains_key(base) && base.starts_with(&format!("{}_", receiver)) {
            return Some(base.to_string());
        }
        None
    }

    /// Allocates and initializes a discriminated-union variant value: a tagged heap block holding
    /// the discriminant at offset 0 and the variant's payload fields at their offsets. Leaves the
    /// new pointer on the stack. Mirrors the auto-generated struct constructor's ownership rules
    /// (borrowed reference arguments are retained; owned ones are taken as-is).
    pub fn build_variant_construction(
        &mut self,
        union_name: &str,
        variant_name: &str,
        args: &[ExpressionNode<'a>],
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let info = self
            .unions
            .get(union_name)
            .ok_or_else(|| Error::UnknownDef(format!("unknown union '{}'", union_name)))?
            .clone();
        let variant = info
            .variant(variant_name)
            .ok_or_else(|| {
                Error::UnknownDef(format!(
                    "union '{}' has no variant '{}'",
                    union_name, variant_name
                ))
            })?
            .clone();

        let base = self.ctor_base_local();
        writer.write_line(&format!("i32.const {}", info.size));
        writer.write_line(&format!("i32.const {}", self.type_tag(union_name)));
        writer.write_line("call $malloc");
        writer.write_line(&format!("local.set {}", base));

        // Store the discriminant at offset 0.
        writer.write_line(&format!("local.get {}", base));
        writer.write_line(&format!("i32.const {}", variant.discriminant));
        writer.write_line("i32.store");

        // Store each payload field (one nesting level deeper for nested allocations).
        self.ctx.alloc_depth += 1;
        for (i, field) in variant.fields.iter().enumerate() {
            let ft = field.type_.get_type();
            let retain_field = match args.get(i) {
                Some(expr) => {
                    self.is_reference_type(&ft) && !self.stores_owned_ref(expr, &ft, function)?
                }
                None => false,
            };
            writer.write_line(&format!("local.get {}", base));
            if field.offset > 0 {
                writer.write_line(&format!("i32.const {}", field.offset));
                writer.write_line("i32.add");
            }
            if let Some(expr) = args.get(i) {
                self.build_expression(expr, &ft, function, writer)?;
            } else {
                writer.write_line("i32.const 0");
            }
            if retain_field {
                writer.write_line("local.tee $scratch_ptr");
                writer.write_line("local.get $scratch_ptr");
                writer.write_line("call $retain");
            }
            WasmGenerator::emit_store(&ft, writer)?;
        }
        self.ctx.alloc_depth -= 1;

        writer.write_line(&format!("local.get {}", base));
        Ok(())
    }

    /// Lowers a `match`. When `value_type` is `Some(t)` the match is in expression position and
    /// every arm yields a value of (Dream) type `t`; when `None` it is a statement and arm bodies
    /// (expressions or blocks) are run for their effects. The subject is evaluated once into a
    /// per-depth `$match_subj{n}` local, then arms become a nested `if/else` over the discriminant.
    pub fn build_match(
        &mut self,
        subject: &ExpressionNode<'a>,
        arms: &[MatchArm<'a>],
        value_type: Option<String>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let raw_subject_type = self.infer_expression_type(subject, function)?;
        let subject_type = self.resolve_type(&raw_subject_type);
        let base_type = strip_nullable(&subject_type).to_string();

        let subj_local = self.match_subj_local();
        self.ctx.match_depth += 1;

        self.build_expression(subject, &subject_type, function, writer)?;
        writer.write_line(&format!("local.set {}", subj_local));

        let arms_ctx = MatchArmsCtx {
            subj_local: &subj_local,
            base_type: &base_type,
            arms,
            value_type: value_type.as_deref(),
            function,
        };
        let result = self.build_match_arms(&arms_ctx, 0, writer);
        self.ctx.match_depth -= 1;
        result
    }

    /// Recursively emits the nested `if/else` chain for `match` arms starting at `index`.
    fn build_match_arms(
        &mut self,
        ctx: &MatchArmsCtx<'a, '_>,
        index: usize,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if index == ctx.arms.len() {
            // Exhaustiveness is enforced by the analyzer, so reaching here is impossible.
            if ctx.value_type.is_some() {
                writer.write_line("unreachable");
            }
            return Ok(());
        }

        let arm = &ctx.arms[index];
        let result_clause = match ctx.value_type {
            Some(vt) => {
                let wasm = WasmGenerator::get_wasm_type_from(self.resolve_type(vt))?;
                format!("(if (result {})", wasm)
            }
            None => "(if".to_string(),
        };

        // Pattern discriminant/literal test (pure: performs no bindings).
        self.emit_pattern_condition(&arm.pattern, ctx.subj_local, &[], None, ctx.base_type, writer)?;
        writer.write_line(&result_clause);
        writer.indent();
        writer.write_line("(then");
        writer.indent();

        // Bind pattern variables now that the pattern matched, so the guard and body can use them.
        self.emit_pattern_bindings(&arm.pattern, ctx.subj_local, &[], None, ctx.base_type, writer)?;

        if let Some(guard) = &arm.guard {
            self.build_expression(guard, &"int".to_string(), ctx.function, writer)?;
            writer.write_line(&result_clause);
            writer.indent();
            writer.write_line("(then");
            writer.indent();
            self.emit_arm_body(arm, ctx.value_type, ctx.function, writer)?;
            writer.unindent();
            writer.write_line(")");
            writer.write_line("(else");
            writer.indent();
            self.build_match_arms(ctx, index + 1, writer)?;
            writer.unindent();
            writer.write_line(")");
            writer.unindent();
            writer.write_line(")");
        } else {
            self.emit_arm_body(arm, ctx.value_type, ctx.function, writer)?;
        }

        writer.unindent();
        writer.write_line(")");
        writer.write_line("(else");
        writer.indent();
        self.build_match_arms(ctx, index + 1, writer)?;
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Emits a single arm's body. In value position the body expression is left on the stack; in
    /// statement position an expression result is dropped/released and a block runs its statements.
    fn emit_arm_body(
        &mut self,
        arm: &MatchArm<'a>,
        value_type: Option<&str>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        match (&arm.body, value_type) {
            (MatchArmBody::Expr(expr), Some(vt)) => {
                self.build_expression(expr, &vt.to_string(), function, writer)
            }
            (MatchArmBody::Expr(expr), None) => {
                let t = self
                    .infer_expression_type(expr, function)
                    .unwrap_or_else(|_| "void".to_string());
                let base = strip_nullable(&t).to_string();
                self.build_expression(expr, &t, function, writer)?;
                if self.is_reference_type(&base) {
                    self.emit_release(&t, writer);
                } else if !base.is_empty() && base != "void" {
                    writer.write_line("drop");
                }
                Ok(())
            }
            (MatchArmBody::Block(stmts), _) => self.build_body(stmts, function, writer),
        }
    }

    /// Emits code that pushes the union/struct pointer reached by `chain` onto the stack.
    fn emit_chain_ptr(&self, subj_local: &str, chain: &[usize], writer: &mut IndentedTextWriter) {
        writer.write_line(&format!("local.get {}", subj_local));
        for off in chain {
            if *off > 0 {
                writer.write_line(&format!("i32.const {}", off));
                writer.write_line("i32.add");
            }
            writer.write_line("i32.load");
        }
    }

    /// Pushes the value currently under a pattern: the pointer/primitive at `parent_chain` (when
    /// `leaf` is `None`) or the typed field `leaf` of that pointer.
    fn emit_pattern_value(
        &self,
        subj_local: &str,
        parent_chain: &[usize],
        leaf: Option<usize>,
        value_type: &str,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        match leaf {
            None => {
                self.emit_chain_ptr(subj_local, parent_chain, writer);
            }
            Some(off) => {
                self.emit_chain_ptr(subj_local, parent_chain, writer);
                if off > 0 {
                    writer.write_line(&format!("i32.const {}", off));
                    writer.write_line("i32.add");
                }
                WasmGenerator::emit_load(strip_nullable(value_type), writer)?;
            }
        }
        Ok(())
    }

    /// Emits an `i32` (1/0) telling whether `pattern` matches the value under it. Reads only; any
    /// bindings are performed separately by `emit_pattern_bindings` once the pattern has matched.
    fn emit_pattern_condition(
        &mut self,
        pattern: &PatternNode,
        subj_local: &str,
        parent_chain: &[usize],
        leaf: Option<usize>,
        value_type: &str,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let base_type = strip_nullable(value_type).to_string();
        match pattern {
            PatternNode::Wildcard(_) => {
                writer.write_line("i32.const 1");
            }
            PatternNode::Binding(name) => {
                // A bare identifier naming a unit variant is a discriminant test; otherwise it is a
                // (always-matching) binding.
                if let Some(disc) = self.unit_variant_discriminant(&base_type, &name.text) {
                    let ptr_chain = cur_ptr_chain(parent_chain, leaf);
                    self.emit_chain_ptr(subj_local, &ptr_chain, writer);
                    writer.write_line("i32.load");
                    writer.write_line(&format!("i32.const {}", disc));
                    writer.write_line("i32.eq");
                } else {
                    writer.write_line("i32.const 1");
                }
            }
            PatternNode::Literal(lit) => {
                self.emit_pattern_value(subj_local, parent_chain, leaf, value_type, writer)?;
                self.build_literal(lit, writer)?;
                if base_type == "string" {
                    writer.write_line("call $string_eq");
                } else {
                    let wasm = WasmGenerator::get_wasm_type_from(self.resolve_type(&base_type))?;
                    writer.write_line(&format!("{}.eq", wasm));
                }
            }
            PatternNode::Variant(_, variant, subs) => {
                let info = match self.unions.get(&base_type).cloned() {
                    Some(info) => info,
                    None => {
                        // Should be caught by the analyzer; emit a never-matching condition.
                        writer.write_line("i32.const 0");
                        return Ok(());
                    }
                };
                let var_info = match info.variant(&variant.text).cloned() {
                    Some(v) => v,
                    None => {
                        writer.write_line("i32.const 0");
                        return Ok(());
                    }
                };

                let ptr_chain = cur_ptr_chain(parent_chain, leaf);
                self.emit_chain_ptr(subj_local, &ptr_chain, writer);
                writer.write_line("i32.load");
                writer.write_line(&format!("i32.const {}", var_info.discriminant));
                writer.write_line("i32.eq");

                if !subs.is_empty() {
                    // Short-circuit: only test sub-patterns when this variant matched (so the
                    // payload fields are valid to read).
                    writer.write_line("(if (result i32)");
                    writer.indent();
                    writer.write_line("(then");
                    writer.indent();
                    for (i, sub) in subs.iter().enumerate() {
                        let field = &var_info.fields[i];
                        self.emit_pattern_condition(
                            sub,
                            subj_local,
                            &ptr_chain,
                            Some(field.offset),
                            &field.type_.get_type(),
                            writer,
                        )?;
                        if i > 0 {
                            writer.write_line("i32.and");
                        }
                    }
                    writer.unindent();
                    writer.write_line(")");
                    writer.write_line("(else i32.const 0)");
                    writer.unindent();
                    writer.write_line(")");
                }
            }
        }
        Ok(())
    }

    /// Performs the bindings introduced by `pattern` (after it has matched): loads each bound
    /// value into its local, retaining reference values so the function-exit release stays
    /// balanced (a bound value is a borrowed view into the subject).
    fn emit_pattern_bindings(
        &mut self,
        pattern: &PatternNode,
        subj_local: &str,
        parent_chain: &[usize],
        leaf: Option<usize>,
        value_type: &str,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let base_type = strip_nullable(value_type).to_string();
        match pattern {
            PatternNode::Wildcard(_) | PatternNode::Literal(_) => {}
            PatternNode::Binding(name) => {
                // Unit-variant patterns bind nothing.
                if self
                    .unit_variant_discriminant(&base_type, &name.text)
                    .is_some()
                {
                    return Ok(());
                }
                self.emit_pattern_value(subj_local, parent_chain, leaf, value_type, writer)?;
                if self.is_reference_type(&base_type) {
                    writer.write_line("local.set $scratch_ptr");
                    writer.write_line("local.get $scratch_ptr");
                    writer.write_line("call $retain");
                    writer.write_line(&format!("local.get ${}", name.text));
                    self.emit_release(value_type, writer);
                    writer.write_line("local.get $scratch_ptr");
                }
                writer.write_line(&format!("local.set ${}", name.text));
            }
            PatternNode::Variant(_, variant, subs) => {
                let info = match self.unions.get(&base_type).cloned() {
                    Some(info) => info,
                    None => return Ok(()),
                };
                let var_info = match info.variant(&variant.text).cloned() {
                    Some(v) => v,
                    None => return Ok(()),
                };
                let ptr_chain = cur_ptr_chain(parent_chain, leaf);
                for (i, sub) in subs.iter().enumerate() {
                    if let Some(field) = var_info.fields.get(i) {
                        self.emit_pattern_bindings(
                            sub,
                            subj_local,
                            &ptr_chain,
                            Some(field.offset),
                            &field.type_.get_type(),
                            writer,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    /// If `type_name` is a union and `name` is one of its unit (payload-less) variants, returns
    /// that variant's discriminant; otherwise `None`.
    fn unit_variant_discriminant(&self, type_name: &str, name: &str) -> Option<i32> {
        let info = self.unions.get(strip_nullable(type_name))?;
        let variant = info.variant(name)?;
        if variant.fields.is_empty() {
            Some(variant.discriminant)
        } else {
            None
        }
    }
}
