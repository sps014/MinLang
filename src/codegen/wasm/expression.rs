use super::WasmGenerator;
use crate::intrinsics;
use crate::syntax::nodes::types::{
    constructor_fn, json_from_json_fn, json_to_json_fn, strip_nullable,
};
use crate::syntax::nodes::{ExpressionNode, FunctionNode, Type};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use std::io::Error;

impl<'a> WasmGenerator<'a> {
    /// Builds an expression, applying implicit boxing when a primitive value flows into an
    /// `object`-typed context (the `left_side`). All other cases defer to `build_expression_inner`.
    pub fn build_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        left_side: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if left_side == "object" {
            let real = self
                .infer_expression_type(expression, function)
                .unwrap_or_else(|_| "object".to_string());
            let base = strip_nullable(&real).to_string();
            if WasmGenerator::is_primitive_name(&base) {
                self.build_expression_inner(expression, &real, function, writer)?;
                writer.write_line(&format!("call $box_{}", base));
                return Ok(());
            }
        }
        self.build_expression_inner(expression, left_side, function, writer)
    }

    /// Builds an expression (no implicit object boxing).
    pub fn build_expression_inner(
        &mut self,
        expression: &ExpressionNode<'a>,
        left_side: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        match expression {
            ExpressionNode::Identifier(identifier) => self.build_identifier(identifier, function, writer)?,
            ExpressionNode::ArrayLiteral(elements) => self.build_array_literal(elements, left_side, function, writer)?,
            ExpressionNode::IndexAccess(array_expr, index_expr) => self.build_index_access(array_expr, index_expr, left_side, function, writer)?,
            ExpressionNode::Unary(opr, expression) => self.build_unary(opr, expression, left_side, function, writer)?,
            ExpressionNode::Binary(left, opr, right) => self.build_binary(left, opr, right, left_side, function, writer)?,
            ExpressionNode::Literal(literal) => self.build_literal(literal, writer)?,
            ExpressionNode::FunctionCall(n, generic_args, args) => {
                match n.text.as_str() {
                    intrinsics::TO_STRING if args.len() == 1 => self.build_to_string(&args[0], function, writer)?,
                    intrinsics::HASH_CODE if args.len() == 1 => self.build_hash_code(&args[0], function, writer)?,
                    intrinsics::ARRAY_NEW if args.len() == 1 => self.build_array_new(generic_args, &args[0], function, writer)?,
                    // The `sleep` async intrinsic produces a `Future` handle directly. The
                    // combinators are invoked as `Promise.all/any/race` (see build_method_call).
                    intrinsics::SLEEP => self.build_async_intrinsic_call(n.text.as_str(), args, function, writer)?,
                    _ => {
                        if let Some((params_decl, ret)) = self.function_typed_local(&n.text, function) {
                            self.build_indirect_call(&n.text, &params_decl, &ret, args, function, writer)?;
                        } else {
                            let function_name = self.resolve_call_name(&n.text, generic_args, args, function);
                            let ctor_name = self.constructor_struct_name(&n.text, generic_args);
                            if self.function_table.get_function(&function_name).is_err()
                                && self.struct_table.get_struct(&ctor_name).is_some() {
                                self.build_constructor(&ctor_name, args, function, writer)?
                            } else {
                                self.build_function_invocation(&function_name, args, function, writer)?
                            }
                        }
                    }
                }
            },
            ExpressionNode::Parenthesized(e) => self.build_expression(e, left_side, function, writer)?,
            ExpressionNode::Cast(target_type, expr) => self.build_cast(target_type, expr, left_side, function, writer)?,
            ExpressionNode::MemberAccess(obj, member) => self.build_member_access(obj, member, left_side, function, writer)?,
            ExpressionNode::IsExpression(left, right_type) => {
                let left_type = self.infer_expression_type(left, function)?;
                if strip_nullable(&left_type) == "object" {
                    // Runtime tag check: the dynamic type of the object is compared to the target.
                    self.build_expression(left, &"object".to_string(), function, writer)?;
                    writer.write_line("call $object_tag");
                    writer.write_line(&format!("i32.const {}", self.type_tag(&right_type.get_type())));
                    writer.write_line("i32.eq");
                } else if left_type == right_type.get_type() {
                    writer.write_line("i32.const 1");
                } else {
                    writer.write_line("i32.const 0");
                }
            },
            ExpressionNode::MethodCall(obj, method, generic_args, params) => self.build_method_call(obj, method, generic_args, params, left_side, function, writer)?,
            ExpressionNode::Ternary(cond, then_e, else_e) => self.build_ternary(cond, then_e, else_e, left_side, function, writer)?,
            // `await` only ever appears at allowed statement positions in v1, which the async
            // statement splitter lowers directly (it never calls `build_expression` on an `Await`).
            ExpressionNode::Await(_) => return Err(Error::other("`await` is only supported at statement position (let x = await e; / await e; / return await e;)")),
        }
        Ok(())
    }

    /// Builds a ternary `cond ? then : else` as a typed `(if (result T) ...)`. Both branches are
    /// emitted with the surrounding expected type so boxing/conversions stay consistent.
    pub fn build_ternary(
        &mut self,
        cond: &ExpressionNode<'a>,
        then_e: &ExpressionNode<'a>,
        else_e: &ExpressionNode<'a>,
        left_side: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let result_wasm = WasmGenerator::get_wasm_type_from(self.resolve_type(left_side))?;
        self.build_expression(cond, &"int".to_string(), function, writer)?;
        writer.write_line(&format!("(if (result {})", result_wasm));
        writer.indent();
        writer.write_line("(then");
        writer.indent();
        self.build_expression(then_e, left_side, function, writer)?;
        writer.unindent();
        writer.write_line(")");
        writer.write_line("(else");
        writer.indent();
        self.build_expression(else_e, left_side, function, writer)?;
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds a type cast expression
    pub fn build_cast(
        &mut self,
        target_type: &Type,
        expr: &ExpressionNode<'a>,
        _left_side: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let target_str = target_type.get_type();
        let source_str = self.infer_expression_type(expr, function)?;
        let target_base = strip_nullable(&target_str).to_string();
        let source_base = strip_nullable(&source_str).to_string();

        // Unboxing: (int)someObject
        if source_base == "object" && WasmGenerator::is_primitive_name(&target_base) {
            self.build_expression_inner(expr, &source_str, function, writer)?;
            writer.write_line(&format!("call $unbox_{}", target_base));
            return Ok(());
        }
        // Boxing: (object)somePrimitive
        if target_base == "object" && WasmGenerator::is_primitive_name(&source_base) {
            self.build_expression_inner(expr, &source_str, function, writer)?;
            writer.write_line(&format!("call $box_{}", source_base));
            return Ok(());
        }

        self.build_expression_inner(expr, &source_str, function, writer)?;

        // Numeric conversions between int/float/double. Same-type casts emit nothing.
        match (source_base.as_str(), target_base.as_str()) {
            ("int", "float") => writer.write_line("f32.convert_i32_s"),
            ("int", "double") => writer.write_line("f64.convert_i32_s"),
            ("float", "int") => writer.write_line("i32.trunc_f32_s"),
            ("double", "int") => writer.write_line("i32.trunc_f64_s"),
            ("float", "double") => writer.write_line("f64.promote_f32"),
            ("double", "float") => writer.write_line("f32.demote_f64"),
            _ => {}
        }
        // For object<->reference and same-type casts the pointer is already correct.

        Ok(())
    }

    /// Allocates a fresh, zero-initialized array of `n` elements (the runtime backing buffer for
    /// `List`/`Map` growth). Layout matches array literals: a `TAG_ARRAY` heap block whose first
    /// word is the element count, followed by the (zeroed) element slots.
    pub fn build_array_new(
        &mut self,
        generic_args: &Option<Vec<Type>>,
        len_expr: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let element_type = generic_args
            .as_ref()
            .and_then(|g| g.first())
            .map(|t| self.resolve_type(&t.get_type()))
            .unwrap_or_else(|| "int".to_string());
        let element_size = WasmGenerator::element_size_of(&element_type);

        // Evaluate the requested length first and stash it. We use dedicated scratch locals
        // ($scratch_len / $scratch_arr) so this never clobbers $scratch_ptr / $scratch_addr, which
        // the enclosing struct instantiation or member/index assignment relies on staying live.
        self.build_expression(len_expr, &"int".to_string(), function, writer)?;
        writer.write_line("local.set $scratch_len");

        // total_size = 4 (length word) + len * element_size
        writer.write_line("i32.const 4");
        writer.write_line("local.get $scratch_len");
        writer.write_line(&format!("i32.const {}", element_size));
        writer.write_line("i32.mul");
        writer.write_line("i32.add");
        writer.write_line(&format!("i32.const {}", super::object::TAG_ARRAY));
        writer.write_line("call $malloc");
        writer.write_line("local.set $scratch_arr");

        // Store the element count at offset 0.
        writer.write_line("local.get $scratch_arr");
        writer.write_line("local.get $scratch_len");
        writer.write_line("i32.store");

        // Zero the element region so unused/leftover slots are null (recycled freelist blocks are
        // not zeroed by the allocator; reference-typed releases rely on null slots).
        writer.write_line("local.get $scratch_arr");
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line("i32.const 0");
        writer.write_line("local.get $scratch_len");
        writer.write_line(&format!("i32.const {}", element_size));
        writer.write_line("i32.mul");
        writer.write_line("memory.fill");

        // Leave the data pointer on the stack.
        writer.write_line("local.get $scratch_arr");
        Ok(())
    }

    /// Builds `len(a)`: the stored slot count of an array (the first word of the buffer).
    pub fn build_len(
        &mut self,
        array_expr: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        self.build_expression(array_expr, &"int[]".to_string(), function, writer)?;
        writer.write_line("i32.load");
        Ok(())
    }

    /// Builds a constructor call `Struct(args)`. Allocates the struct then either runs the
    /// user-defined `constructor` method (custom constructor) or stores the arguments positionally
    /// into the fields in declaration order (auto-generated constructor). Leaves the new pointer on
    /// the stack. `struct_name` is already monomorphized (e.g. `Point_int`).
    pub fn build_constructor(
        &mut self,
        struct_name: &str,
        args: &Vec<ExpressionNode<'a>>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let struct_info = self
            .struct_table
            .get_struct(struct_name)
            .ok_or_else(|| Error::other(format!("unknown class '{}' in constructor", struct_name)))?
            .clone();
        let init_name = constructor_fn(struct_name);
        let has_init = self.function_table.get_function(&init_name).is_ok();

        // Allocate, tagging the block with this struct's runtime tag, into a depth-specific local.
        let base = self.ctor_base_local();
        writer.write_line(&format!("i32.const {}", struct_info.size));
        writer.write_line(&format!("i32.const {}", self.type_tag(struct_name)));
        writer.write_line("call $malloc");
        writer.write_line(&format!("local.set {}", base));

        let ordered = WasmGenerator::sorted_fields(&struct_info);

        if has_init {
            // Zero every field before running `constructor` (reused heap blocks are not zeroed), so
            // a `constructor` that leaves some fields unset observes 0/null rather than stale data.
            for (_, field_info) in &ordered {
                let ft = field_info.type_.get_type();
                writer.write_line(&format!("local.get {}", base));
                if field_info.offset > 0 {
                    writer.write_line(&format!("i32.const {}", field_info.offset));
                    writer.write_line("i32.add");
                }
                match WasmGenerator::get_wasm_type_from(ft.clone())?.as_str() {
                    "f64" => writer.write_line("f64.const 0"),
                    "f32" => writer.write_line("f32.const 0"),
                    _ => writer.write_line("i32.const 0"),
                }
                WasmGenerator::emit_store(&ft, writer)?;
            }

            // call $Struct_constructor(this, args...). Owned argument temporaries are released after
            // the call (constructor returns void, so nothing is on the stack to disturb).
            let param_types: Vec<String> = self
                .function_table
                .get_function(&init_name)?
                .parameters
                .clone();
            let saved_tmp = self.ctx.tmp_depth;
            let mut owned_temps: Vec<(usize, String)> = Vec::new();
            writer.write_line(&format!("local.get {}", base)); // implicit `this`
            self.ctx.alloc_depth += 1;
            for (i, expr) in args.iter().enumerate() {
                let pt = param_types
                    .get(i + 1)
                    .cloned()
                    .unwrap_or_else(|| "int".to_string());
                self.build_call_arg(expr, &pt, function, &mut owned_temps, writer)?;
            }
            self.ctx.alloc_depth -= 1;
            writer.write_line(&format!("call ${}", init_name));
            self.release_call_temps(&owned_temps, saved_tmp, writer);
        } else {
            // Auto-generated constructor: store each argument into its field positionally. The
            // struct owns its reference fields, so a borrowed value is retained while an owned one
            // (constructor/literal/call result, or a boxed primitive) is taken as-is.
            self.ctx.alloc_depth += 1;
            for (i, (_, field_info)) in ordered.iter().enumerate() {
                let ft = field_info.type_.get_type();
                let retain_field = match args.get(i) {
                    Some(expr) => {
                        self.is_reference_type(&ft)
                            && !self.stores_owned_ref(expr, &ft, function)?
                    }
                    None => false,
                };
                writer.write_line(&format!("local.get {}", base));
                if field_info.offset > 0 {
                    writer.write_line(&format!("i32.const {}", field_info.offset));
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
        }

        writer.write_line(&format!("local.get {}", base));
        Ok(())
    }

    /// Builds a member access
    pub fn build_member_access(
        &mut self,
        obj: &ExpressionNode<'a>,
        member: &SyntaxToken,
        _left_side: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // Enum member access `EnumName.Member` lowers to the member's integer constant.
        if let ExpressionNode::Identifier(id) = obj {
            if let Some(members) = self.enums.get(&id.text) {
                let value = members.get(&member.text).copied().ok_or_else(|| {
                    Error::other(format!(
                        "enum '{}' has no member '{}'",
                        id.text, member.text
                    ))
                })?;
                writer.write_line(&format!("i32.const {}", value));
                return Ok(());
            }
        }
        let obj_type_str = self.infer_expression_type(obj, function)?;
        let base_obj_type_str = strip_nullable(&obj_type_str).to_string();
        let struct_info = self
            .struct_table
            .get_struct(&base_obj_type_str)
            .ok_or_else(|| {
                Error::other(format!(
                    "unknown class '{}' in member access",
                    base_obj_type_str
                ))
            })?
            .clone();
        let field_info = struct_info.fields.get(&member.text).ok_or_else(|| {
            Error::other(format!(
                "unknown field '{}' on class '{}'",
                member.text, base_obj_type_str
            ))
        })?;
        let offset = field_info.offset;
        let field_type = field_info.type_.get_type();

        self.build_expression(obj, &obj_type_str, function, writer)?; // ptr

        if offset > 0 {
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add"); // ptr + offset
        }

        WasmGenerator::emit_load(&field_type, writer)?;
        Ok(())
    }

    /// Builds a literal value
    pub fn build_literal(
        &mut self,
        literal: &Type,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let type_ = match literal {
            Type::Integer(i) => format!("i32.const {}", i.text),
            Type::Float(f) => format!("f32.const {}", f.text),
            Type::Double(d) => format!("f64.const {}", d.text),
            Type::Boolean(f) => format!("i32.const {}", if f.text == "true" { 1 } else { 0 }),
            // A char literal's token text already holds its numeric code point.
            Type::Char(c) => format!("i32.const {}", c.text),
            Type::String(s) => {
                let offset = self.ctx.strings.get(&s.text).ok_or_else(|| {
                    Error::other(format!("string literal not interned: {}", s.text))
                })?;
                format!("i32.const {}", offset)
            }
            Type::Nullable(_) => "i32.const 0".to_string(),
            _ => return Err(Error::other(format!("unknown literal {:?}", literal))),
        };
        writer.write_line(&type_);
        Ok(())
    }

    /// Builds a binary expression
    pub fn build_binary(
        &mut self,
        left_exp: &ExpressionNode<'a>,
        opr: &SyntaxToken,
        right_expr: &ExpressionNode<'a>,
        left: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // Short-circuit logical operators: the right operand is only evaluated when its result can
        // still affect the outcome. `&&` -> `if left then right else 0`; `||` -> `if left then 1
        // else right`. The eager bitwise `&`/`|` operators are handled in the generic path below.
        // Null-coalescing `a ?? b`: evaluate `a` once into a scratch local; if it is non-null
        // (pointer != 0) yield it, otherwise evaluate and yield `b`.
        if opr.kind == TokenKind::QuestionQuestionToken {
            self.build_expression(left_exp, left, function, writer)?;
            writer.write_line("local.tee $scratch_coalesce");
            writer.write_line("i32.const 0");
            writer.write_line("i32.ne");
            writer.write_line("(if (result i32)");
            writer.indent();
            writer.write_line("(then");
            writer.indent();
            writer.write_line("local.get $scratch_coalesce");
            writer.unindent();
            writer.write_line(")");
            writer.write_line("(else");
            writer.indent();
            self.build_expression(right_expr, left, function, writer)?;
            writer.unindent();
            writer.write_line(")");
            writer.unindent();
            writer.write_line(")");
            return Ok(());
        }

        if matches!(
            opr.kind,
            TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken
        ) {
            self.build_expression(left_exp, left, function, writer)?;
            writer.write_line("(if (result i32)");
            writer.indent();
            if opr.kind == TokenKind::AmpersandAmpersandToken {
                writer.write_line("(then");
                writer.indent();
                self.build_expression(right_expr, left, function, writer)?;
                writer.unindent();
                writer.write_line(")");
                writer.write_line("(else (i32.const 0))");
            } else {
                writer.write_line("(then (i32.const 1))");
                writer.write_line("(else");
                writer.indent();
                self.build_expression(right_expr, left, function, writer)?;
                writer.unindent();
                writer.write_line(")");
            }
            writer.unindent();
            writer.write_line(")");
            return Ok(());
        }

        // For a comparison the surrounding `left` is the bool/int result context, not the operand
        // type, so evaluate the operands (and pick the comparison instruction) in their own type.
        // This makes float/double/char comparisons emit `f32.lt`/`f64.lt`/... instead of `i32.lt_s`.
        let is_comparison = matches!(
            opr.kind,
            TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken
                | TokenKind::GreaterThanToken
                | TokenKind::GreaterThanEqualToken
                | TokenKind::SmallerThanToken
                | TokenKind::SmallerThanEqualToken
        );
        let operand_ctx = if is_comparison {
            let t = self
                .infer_expression_type(left_exp, function)
                .unwrap_or_else(|_| left.clone());
            strip_nullable(&t).to_string()
        } else {
            left.clone()
        };

        self.build_expression(left_exp, &operand_ctx, function, writer)?;
        self.build_expression(right_expr, &operand_ctx, function, writer)?;

        if left == "string" && opr.kind == TokenKind::PlusToken {
            writer.write_line("call $concat_strings");
            return Ok(());
        }

        // String equality compares contents, not pointers. Detect via the operand type (the
        // expression's own `left_side` is `bool`/`int` for a comparison, so it cannot be used).
        if matches!(
            opr.kind,
            TokenKind::EqualEqualToken | TokenKind::NotEqualToken
        ) {
            let operand_type = self
                .infer_expression_type(left_exp, function)
                .unwrap_or_else(|_| left.clone());
            if strip_nullable(&operand_type) == "string" {
                writer.write_line("call $string_eq");
                if opr.kind == TokenKind::NotEqualToken {
                    writer.write_line("i32.eqz");
                }
                return Ok(());
            }
        }

        let symbol = WasmGenerator::get_wasm_type_from(operand_ctx.clone())?;
        match opr.kind {
            TokenKind::PlusToken => writer.write_line(&format!("{}.add", symbol)),
            TokenKind::MinusToken => writer.write_line(&format!("{}.sub", symbol)),
            TokenKind::StarToken => writer.write_line(&format!("{}.mul", symbol)),
            TokenKind::EqualEqualToken => writer.write_line(&format!("{}.eq", symbol)),
            TokenKind::NotEqualToken => writer.write_line(&format!("{}.ne", symbol)),
            _ => {}
        };

        if symbol == "f32" {
            match opr.kind {
                TokenKind::SlashToken => writer.write_line(&format!("{}.div", symbol)),
                TokenKind::ModulusToken => writer.write_line(&format!("{}.rem", symbol)),
                TokenKind::GreaterThanToken => writer.write_line(&format!("{}.gt", symbol)),
                TokenKind::SmallerThanToken => writer.write_line(&format!("{}.lt", symbol)),
                TokenKind::GreaterThanEqualToken => writer.write_line(&format!("{}.ge", symbol)),
                TokenKind::SmallerThanEqualToken => writer.write_line(&format!("{}.le", symbol)),
                TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::StarToken
                | TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken => {}
                _ => return Err(Error::other(format!("unknown operator {}", opr.text))),
            };
        } else if symbol == "f64" {
            match opr.kind {
                TokenKind::SlashToken => writer.write_line(&format!("{}.div", symbol)),
                TokenKind::ModulusToken => {
                    return Err(Error::other("modulus not supported for double"))
                }
                TokenKind::GreaterThanToken => writer.write_line(&format!("{}.gt", symbol)),
                TokenKind::SmallerThanToken => writer.write_line(&format!("{}.lt", symbol)),
                TokenKind::GreaterThanEqualToken => writer.write_line(&format!("{}.ge", symbol)),
                TokenKind::SmallerThanEqualToken => writer.write_line(&format!("{}.le", symbol)),
                TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::StarToken
                | TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken => {}
                _ => return Err(Error::other(format!("unknown operator {}", opr.text))),
            };
        } else if symbol == "i32" {
            match opr.kind {
                TokenKind::SlashToken => writer.write_line(&format!("{}.div_s", symbol)),
                TokenKind::ModulusToken => writer.write_line(&format!("{}.rem_s", symbol)),
                TokenKind::GreaterThanToken => writer.write_line(&format!("{}.gt_s", symbol)),
                TokenKind::SmallerThanToken => writer.write_line(&format!("{}.lt_s", symbol)),
                TokenKind::GreaterThanEqualToken => writer.write_line(&format!("{}.ge_s", symbol)),
                TokenKind::SmallerThanEqualToken => writer.write_line(&format!("{}.le_s", symbol)),
                TokenKind::AmpersandAmpersandToken | TokenKind::BitWiseAmpersandToken => {
                    writer.write_line(&format!("{}.and", symbol))
                }
                TokenKind::PipePipeToken | TokenKind::BitWisePipeToken => {
                    writer.write_line(&format!("{}.or", symbol))
                }
                TokenKind::BitWiseXorToken => writer.write_line(&format!("{}.xor", symbol)),
                TokenKind::ShiftLeftToken => writer.write_line(&format!("{}.shl", symbol)),
                TokenKind::ShiftRightToken => writer.write_line(&format!("{}.shr_s", symbol)),
                TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::StarToken
                | TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken => {}
                _ => return Err(Error::other(format!("unknown operator {}", opr.text))),
            };
        } else {
            return Err(Error::other(format!("unknown symbol {}", symbol)));
        }

        Ok(())
    }

    /// Builds a unary expression
    pub fn build_unary(
        &mut self,
        opr: &SyntaxToken,
        expression: &ExpressionNode<'a>,
        left: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        self.build_expression(expression, left, function, writer)?;
        let symbol = WasmGenerator::get_wasm_type_from(left.clone())?;
        match opr.kind {
            TokenKind::MinusToken => {
                writer.write_line(&format!("{}.const -1", symbol));
                writer.write_line(&format!("{}.mul", symbol));
            }
            TokenKind::BangToken => {
                writer.write_line(&format!("{}.const 0", symbol));
                writer.write_line(&format!("{}.eq", symbol));
            }
            TokenKind::PlusToken => {}
            _ => {
                return Err(Error::other(format!(
                    "wasm does not support unary operator {}",
                    opr.text
                )))
            }
        };
        Ok(())
    }

    /// Builds an identifier reference
    pub fn build_identifier(
        &mut self,
        identifier: &SyntaxToken,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // A bare identifier that is not a local variable but names a top-level function is a
        // first-class function value: emit its function-table index.
        let func_name = self
            .ctx
            .current_mangled_name
            .as_ref()
            .unwrap_or(&function.name.text);
        let is_local = self
            .ctx
            .combined_symbol_lookup
            .get(func_name)
            .map(|m| m.contains_key(&identifier.text))
            .unwrap_or(false);
        if !is_local {
            // A top-level variable lives in a WASM global, not a function local.
            if self.ctx.globals.contains_key(&identifier.text) {
                writer.write_line(&format!("global.get ${}", identifier.text));
                return Ok(());
            }
            if let Some(idx) = self.ctx.function_indices.get(&identifier.text) {
                writer.write_line(&format!("i32.const {}", idx));
                return Ok(());
            }
        }
        writer.write_line(&format!("local.get ${}", identifier.text));
        Ok(())
    }

    /// Returns the structured signature `(param types, return type)` when `var_name` is a local
    /// variable of function type in the current function, otherwise `None`.
    pub fn function_typed_local(
        &self,
        var_name: &str,
        function: &FunctionNode<'a>,
    ) -> Option<(Vec<Type>, Type)> {
        let func_name = self
            .ctx
            .current_mangled_name
            .as_ref()
            .unwrap_or(&function.name.text);
        let t = self
            .ctx
            .combined_symbol_lookup
            .get(func_name)?
            .get(var_name)?;
        if let Type::Function(params, ret) = t {
            Some((params.clone(), (**ret).clone()))
        } else {
            None
        }
    }

    /// Builds an indirect call through a function-typed local variable using `call_indirect`.
    /// The variable holds an `i32` index into the module's function table.
    pub fn build_indirect_call(
        &mut self,
        var_name: &str,
        params_decl: &[Type],
        ret: &Type,
        args: &Vec<ExpressionNode<'a>>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let saved_tmp = self.ctx.tmp_depth;
        let mut owned_temps: Vec<(usize, String)> = Vec::new();
        for (i, expr) in args.iter().enumerate() {
            let pt = params_decl
                .get(i)
                .map(|t| t.get_type())
                .unwrap_or_else(|| "int".to_string());
            self.build_call_arg(expr, &pt, function, &mut owned_temps, writer)?;
        }
        // Push the table index held by the variable, then dispatch.
        writer.write_line(&format!("local.get ${}", var_name));

        let mut param_str = String::new();
        for p in params_decl {
            param_str.push_str(&WasmGenerator::get_wasm_type_from(
                self.resolve_type(&p.get_type()),
            )?);
            param_str.push(' ');
        }
        let mut sig = String::new();
        if !param_str.trim().is_empty() {
            sig.push_str(&format!("(param {})", param_str.trim()));
        }
        if !matches!(ret, Type::Void) {
            let ret_wasm = WasmGenerator::get_wasm_type_from(self.resolve_type(&ret.get_type()))?;
            if !ret_wasm.is_empty() {
                if !sig.is_empty() {
                    sig.push(' ');
                }
                sig.push_str(&format!("(result {})", ret_wasm));
            }
        }
        if sig.is_empty() {
            writer.write_line("call_indirect");
        } else {
            writer.write_line(&format!("call_indirect {}", sig));
        }
        self.release_call_temps(&owned_temps, saved_tmp, writer);
        Ok(())
    }

    /// Builds a function invocation
    pub fn build_function_invocation(
        &mut self,
        name: &String,
        parameters: &Vec<ExpressionNode<'a>>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let param_types: Vec<String> = self.function_table.get_function(name)?.parameters.clone();

        let saved_tmp = self.ctx.tmp_depth;
        let mut owned_temps: Vec<(usize, String)> = Vec::new();
        for (i, expr) in parameters.iter().enumerate() {
            let param_type = param_types
                .get(i)
                .cloned()
                .unwrap_or_else(|| "int".to_string()); // Fallback if arity mismatch (caught by semantic analysis)
            self.build_call_arg(expr, &param_type, function, &mut owned_temps, writer)?;
        }
        writer.write("call $");
        writer.write_line(name);
        self.release_call_temps(&owned_temps, saved_tmp, writer);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_method_call(
        &mut self,
        obj: &ExpressionNode<'a>,
        method: &SyntaxToken,
        _generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        _left_side: &String,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // `Type.method(args)`: a static call. Arguments map 1:1 to the parameters (no `this`).
        if let Some(key) = self.resolve_static_call(obj, &method.text, params, function) {
            let info = self.function_table.get_function(&key)?;
            if let Some(intrinsic) = &info.intrinsic_name {
                match intrinsic.as_str() {
                    "print" => {
                        return self.build_print(&params[0], function, writer);
                    }
                    "println" => {
                        return self.build_println(&params[0], function, writer);
                    }
                    "promise_all" => {
                        return self.build_async_intrinsic_call(intrinsics::PROMISE_ALL, params, function, writer);
                    }
                    "promise_any" => {
                        return self.build_async_intrinsic_call(intrinsics::PROMISE_ANY, params, function, writer);
                    }
                    "promise_race" => {
                        return self.build_async_intrinsic_call(intrinsics::PROMISE_RACE, params, function, writer);
                    }
                    "json_serialize" => {
                        let arg_type = self.infer_expression_type(&params[0], function)?;
                        let struct_name = strip_nullable(&arg_type).to_string();
                        self.build_expression(&params[0], &arg_type, function, writer)?;
                        writer.write_line(&format!("call ${}", json_to_json_fn(&struct_name)));
                        writer.write_line("call $JSON_stringify");
                        return Ok(());
                    }
                    "json_deserialize" => {
                        let struct_name = _generic_args
                            .as_ref()
                            .and_then(|g| g.first())
                            .map(|t| strip_nullable(&t.get_type()).to_string())
                            .unwrap_or_default();
                        self.build_expression(&params[0], &"string".to_string(), function, writer)?;
                        writer.write_line("call $JSON_parse");
                        writer.write_line(&format!("call ${}", json_from_json_fn(&struct_name)));
                        return Ok(());
                    }
                    _ => {}
                }
            }

            let param_types: Vec<String> = info.parameters.clone();
            let saved_tmp = self.ctx.tmp_depth;
            let mut owned_temps: Vec<(usize, String)> = Vec::new();
            for (i, expr) in params.iter().enumerate() {
                let param_type = param_types
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| "int".to_string());
                self.build_call_arg(expr, &param_type, function, &mut owned_temps, writer)?;
            }
            writer.write_line(&format!("call ${}", key));
            self.release_call_temps(&owned_temps, saved_tmp, writer);
            return Ok(());
        }

        let obj_type = self.infer_expression_type(obj, function)?;

        // `EnumValue.name()`: map an enum's integer value to its variant-name string via the
        // per-enum `$enum_name_*` lookup function.
        if method.text == intrinsics::ENUM_NAME {
            let base = strip_nullable(&obj_type).to_string();
            if self.enums.contains_key(&base) {
                self.build_expression(obj, &obj_type, function, writer)?;
                writer.write_line(&format!("call $enum_name_{}", base));
                return Ok(());
            }
        }

        // `arr.len()` / `str.len()`: length of an array (stored count) or string (`$strlen`).
        if method.text == intrinsics::LEN {
            let base = strip_nullable(&obj_type).to_string();
            if base.ends_with("[]") {
                self.build_len(obj, function, writer)?;
                return Ok(());
            }
            if base == "string" {
                self.build_expression(obj, &obj_type, function, writer)?;
                writer.write_line("call $strlen");
                return Ok(());
            }
        }

        let struct_name = strip_nullable(&obj_type).to_string();
        // Resolve the (possibly overloaded) method to its emitted variant, matching the analyzer.
        let mangled_name = self.resolve_method_key(&struct_name, &method.text, params, function);
        let param_types: Vec<String> = self
            .function_table
            .get_function(&mangled_name)?
            .parameters
            .clone();

        let saved_tmp = self.ctx.tmp_depth;
        let mut owned_temps: Vec<(usize, String)> = Vec::new();

        // 1. Evaluate 'this' (the object). If the receiver is itself an owned temporary (e.g. a
        // chained call result like `makePoint().dist()`), stash it for release after the call.
        let recv_owned =
            self.is_reference_type(&struct_name) && self.produces_owned_ref(obj, function);
        self.build_expression(obj, &obj_type, function, writer)?;
        if recv_owned {
            let slot = self.ctx.tmp_depth.min(Self::TMP_POOL - 1);
            self.ctx.tmp_depth += 1;
            writer.write_line(&format!("local.tee $tmp{}", slot));
            owned_temps.push((slot, struct_name.clone()));
        }

        // 2. Evaluate remaining parameters (index +1 because 'this' is at index 0).
        for (i, expr) in params.iter().enumerate() {
            let param_type = param_types
                .get(i + 1)
                .cloned()
                .unwrap_or_else(|| "int".to_string());
            self.build_call_arg(expr, &param_type, function, &mut owned_temps, writer)?;
        }

        writer.write_line(&format!("call ${}", mangled_name));
        self.release_call_temps(&owned_temps, saved_tmp, writer);
        Ok(())
    }

    /// Builds an array literal
    pub fn build_array_literal(
        &mut self,
        elements: &Vec<ExpressionNode<'a>>,
        left_side: &str,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let len = elements.len();

        let inner_type_str = if let Some(stripped) = left_side.strip_suffix("[]") {
            stripped.to_string()
        } else {
            "int".to_string() // Fallback, shouldn't happen if semantic analysis is correct
        };

        let element_size = WasmGenerator::element_size_of(&inner_type_str);
        let total_size = 4 + (len * element_size); // 4 bytes for length + element_size per element

        // 1. Allocate the backing buffer (tagged as an array). Hold its pointer in a
        // depth-specific local so evaluating element values (which may themselves allocate)
        // cannot clobber it.
        let base = self.ctor_base_local();
        writer.write_line(&format!("i32.const {}", total_size));
        writer.write_line(&format!("i32.const {}", super::object::TAG_ARRAY));
        writer.write_line("call $malloc");
        writer.write_line(&format!("local.set {}", base));

        // 2. Store the element count at offset 0.
        writer.write_line(&format!("local.get {}", base));
        writer.write_line(&format!("i32.const {}", len));
        writer.write_line("i32.store");

        // 3. Evaluate and store each element (one nesting level deeper).
        self.ctx.alloc_depth += 1;
        for (i, expr) in elements.iter().enumerate() {
            let offset = 4 + (i * element_size);
            writer.write_line(&format!("local.get {}", base)); // ptr
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add"); // ptr + offset

            // The array owns its reference elements: retain a borrowed value, take an owned one.
            let retain_elem = self.is_reference_type(&inner_type_str)
                && !self.stores_owned_ref(expr, &inner_type_str, function)?;
            self.build_expression(expr, &inner_type_str, function, writer)?;
            if retain_elem {
                writer.write_line("local.tee $scratch_ptr");
                writer.write_line("local.get $scratch_ptr");
                writer.write_line("call $retain");
            }
            WasmGenerator::emit_store(&inner_type_str, writer)?;
        }
        self.ctx.alloc_depth -= 1;

        // 4. Leave the pointer on the stack
        writer.write_line(&format!("local.get {}", base));
        Ok(())
    }

    /// Builds an array index access
    pub fn build_index_access(
        &mut self,
        array_expr: &ExpressionNode<'a>,
        index_expr: &ExpressionNode<'a>,
        left_side: &str,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // Here left_side is the expected type of the expression, which is the inner type of the array
        let element_size = WasmGenerator::element_size_of(left_side);

        // Calculate the memory address: ptr + 4 + (index * element_size)
        // Note: We pass a dummy type "int[]" to build_expression for the array ptr because we just need an i32 back
        self.build_expression(array_expr, &"int[]".to_string(), function, writer)?; // ptr
        writer.write_line("i32.const 4");
        writer.write_line("i32.add"); // ptr + 4

        self.build_expression(index_expr, &"int".to_string(), function, writer)?; // index
        if element_size != 1 {
            writer.write_line(&format!("i32.const {}", element_size));
            writer.write_line("i32.mul"); // index * element_size
        }

        writer.write_line("i32.add"); // ptr + 4 + (index * element_size)

        WasmGenerator::emit_load(left_side, writer)?;
        Ok(())
    }
}
