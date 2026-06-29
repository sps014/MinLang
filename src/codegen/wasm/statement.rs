use super::WasmGenerator;
use crate::intrinsics;
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, StatementNode, Type};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use crate::syntax::token::syntax_token::SyntaxToken;
use std::io::Error;

impl<'a> WasmGenerator<'a> {
    /// Builds the body of a function
    pub fn build_body(
        &mut self,
        statements: &[StatementNode<'a>],
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        for i in statements.iter() {
            self.build_statement(i, function, writer)?;
        }
        Ok(())
    }

    /// Builds a single statement
    pub fn build_statement(
        &mut self,
        statement: &StatementNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        match statement {
            StatementNode::Declaration(left, _, expression, _) => {
                self.build_declaration(left, function, expression, writer)?
            }
            StatementNode::Assignment(left, expression) => {
                self.build_assignment(left, expression, function, writer)?
            }
            StatementNode::IndexAssignment(left, index, expression) => {
                self.build_index_assignment(left, index, expression, function, writer)?
            }
            StatementNode::MemberAssignment(obj, member, expression) => {
                self.build_member_assignment(obj, member, expression, function, writer)?
            }
            StatementNode::Return(r) => self.build_return(r, function, writer)?,
            StatementNode::While(c, b) => self.build_while(c, b, function, writer)?,
            StatementNode::DoWhile(b, c) => self.build_do_while(b, c, function, writer)?,
            StatementNode::For(init, cond, inc, body) => {
                self.build_for(init, cond, inc, body, function, writer)?
            }
            StatementNode::ForEach(element, iterable, index_name, array_name, body) => self
                .build_foreach(
                    element, iterable, index_name, array_name, body, function, writer,
                )?,
            StatementNode::Switch(subject, cases, default_body) => {
                self.build_switch(subject, cases, default_body, function, writer)?
            }
            StatementNode::Labeled(label, inner) => {
                // The next loop construct adopts this label for targeted break/continue.
                self.ctx.pending_loop_label = Some(label.clone());
                self.build_statement(inner, function, writer)?;
                self.ctx.pending_loop_label = None;
            }
            StatementNode::Break(label) => self.build_break(label, writer)?,
            StatementNode::Continue(label) => self.build_continue(label, writer)?,
            StatementNode::IfElse(c, b, else_if, else_b) => {
                self.build_if_else(c, b, else_if, else_b, function, writer)?
            }
            StatementNode::AwaitStmt(child) => {
                // A bare `await e;` outside the async statement splitter is unreachable for valid
                // v1 programs (the splitter rewrites top-level awaits). Defensively evaluate the
                // future for its eager side effects and discard it.
                self.build_expression(child, &"int".to_string(), function, writer)?;
                writer.write_line("drop");
            }
            StatementNode::FunctionInvocation(n, generic_args, p) => {
                match n.text.as_str() {
                    intrinsics::SLEEP => {
                        // A discarded async intrinsic call: build the future and drop the handle.
                        self.build_async_intrinsic_call(n.text.as_str(), p, function, writer)?;
                        writer.write_line("drop");
                    }
                    intrinsics::PRINT if p.len() == 1 => {
                        self.build_print(&p[0], function, writer)?
                    }
                    intrinsics::PRINTLN if p.len() == 1 => {
                        self.build_println(&p[0], function, writer)?
                    }
                    intrinsics::TO_STRING if p.len() == 1 => {
                        self.build_to_string(&p[0], function, writer)?;
                        writer.write_line("drop");
                    }
                    intrinsics::HASH_CODE if p.len() == 1 => {
                        self.build_hash_code(&p[0], function, writer)?;
                        writer.write_line("drop");
                    }
                    _ => {
                        if let Some((params_decl, ret)) =
                            self.function_typed_local(&n.text, function)
                        {
                            self.build_indirect_call(
                                &n.text,
                                &params_decl,
                                &ret,
                                p,
                                function,
                                writer,
                            )?;
                            // The callee returns an owned +1: release a reference result, drop a
                            // plain value, ignore void.
                            let ret_str = ret.get_type();
                            if self.is_reference_type(strip_nullable(&ret_str)) {
                                self.emit_release(&ret_str, writer);
                            } else if !matches!(ret, Type::Void) {
                                writer.write_line("drop");
                            }
                        } else {
                            let function_name =
                                self.resolve_call_name(&n.text, generic_args, p, function);
                            let ctor_name = self.constructor_struct_name(&n.text, generic_args);
                            if self.function_table.get_function(&function_name).is_err()
                                && self.struct_table.get_struct(&ctor_name).is_some()
                            {
                                // Constructed value is discarded: build it, then release the fresh
                                // allocation (which also balances the stack).
                                self.build_constructor(&ctor_name, p, function, writer)?;
                                self.emit_release(&ctor_name, writer);
                            } else {
                                // A discarded function result is an owned +1: release a reference,
                                // drop a plain value, ignore void.
                                let ret_str = self
                                    .function_table
                                    .get_function(&function_name)
                                    .ok()
                                    .and_then(|f| f.return_type)
                                    .map(|t| t.get_type());
                                self.build_function_invocation(
                                    &function_name,
                                    p,
                                    function,
                                    writer,
                                )?;
                                match ret_str {
                                    Some(t) if self.is_reference_type(strip_nullable(&t)) => {
                                        self.emit_release(&t, writer)
                                    }
                                    Some(t) if t != "void" => writer.write_line("drop"),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            StatementNode::MethodInvocation(obj, method, generic_args, params) => {
                // Called purely for side effects: reclaim/discard any returned value so the WASM
                // stack stays balanced. A user method returns an owned +1 (release it); builtin
                // `.name()` returns a borrowed interned string and `.len()` an int (just drop).
                let ret = self.method_return_type(obj, method, params, function)?;
                self.build_method_call(
                    obj,
                    method,
                    generic_args,
                    params,
                    &"void".to_string(),
                    function,
                    writer,
                )?;
                match ret {
                    Some(t)
                        if self.is_reference_type(strip_nullable(&t))
                            && method.text != intrinsics::ENUM_NAME =>
                    {
                        self.emit_release(&t, writer)
                    }
                    Some(t) if t != "void" => writer.write_line("drop"),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Builds a variable declaration
    pub fn build_declaration(
        &mut self,
        left: &SyntaxToken,
        function: &FunctionNode<'a>,
        expression: &ExpressionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let type_str = self.table_read_type(&left.text, function);
        // An owned reference (constructor/literal/call result, or a boxed primitive) already
        // carries the single reference this binding takes over, so it must not be retained again
        // (otherwise its refcount never reaches 0 and `drop` never runs). A borrowed value must
        // be retained.
        let owns_ref = self.stores_owned_ref(expression, &type_str, function)?;
        self.build_expression(expression, &type_str, function, writer)?;

        if self.is_reference_type(&type_str) {
            writer.write_line("local.set $scratch_ptr");
            if !owns_ref {
                writer.write_line("local.get $scratch_ptr");
                writer.write_line("call $retain");
            }

            // Release old value (in case this declaration is inside a loop and the local is reused).
            writer.write_line(&format!("local.get ${}", left.text));
            self.emit_release(&type_str, writer);

            writer.write_line("local.get $scratch_ptr");
        }

        writer.write_line(&format!("local.set ${}", left.text));
        Ok(())
    }

    /// Builds a variable assignment
    pub fn build_assignment(
        &mut self,
        left: &SyntaxToken,
        expression: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let type_str = self.table_read_type(&left.text, function);

        // An owned reference already carries the reference being assigned; do not retain it again.
        let owns_ref = self.stores_owned_ref(expression, &type_str, function)?;
        self.build_expression(expression, &type_str, function, writer)?;

        if self.is_reference_type(&type_str) {
            writer.write_line("local.set $scratch_ptr");

            // Retain the new value (unless it already owns its reference), then release the value
            // previously held by this local.
            if !owns_ref {
                writer.write_line("local.get $scratch_ptr");
                writer.write_line("call $retain");
            }
            writer.write_line(&format!("local.get ${}", left.text));
            self.emit_release(&type_str, writer);

            writer.write_line("local.get $scratch_ptr");
        }

        writer.write_line(&format!("local.set ${}", left.text));
        Ok(())
    }

    /// Builds an array index assignment
    pub fn build_index_assignment(
        &mut self,
        arr: &ExpressionNode<'a>,
        index: &ExpressionNode<'a>,
        expression: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let array_type_str = self.infer_expression_type(arr, function)?;
        let inner_type_str = array_type_str[..array_type_str.len() - 2].to_string();
        let element_size = WasmGenerator::element_size_of(&inner_type_str);

        // Calculate the memory address: ptr + 4 + (index * element_size)
        self.build_expression(arr, &array_type_str, function, writer)?;
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        self.build_expression(index, &"int".to_string(), function, writer)?;
        if element_size != 1 {
            writer.write_line(&format!("i32.const {}", element_size));
            writer.write_line("i32.mul");
        }
        writer.write_line("i32.add");
        writer.write_line("local.set $scratch_addr");

        self.emit_store_with_refcount(&inner_type_str, expression, function, writer)?;
        WasmGenerator::emit_store(&inner_type_str, writer)?;
        Ok(())
    }

    /// Builds a member assignment
    pub fn build_member_assignment(
        &mut self,
        obj: &ExpressionNode<'a>,
        member: &SyntaxToken,
        expression: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let obj_type_str = self.infer_expression_type(obj, function)?;
        let base_obj_type_str = if obj_type_str.ends_with("?") {
            obj_type_str[..obj_type_str.len() - 1].to_string()
        } else {
            obj_type_str.clone()
        };
        let struct_info = self
            .struct_table
            .get_struct(&base_obj_type_str)
            .ok_or_else(|| {
                Error::other(format!(
                    "unknown class '{}' in member assignment",
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
        let field_type_str = field_info.type_.get_type();

        // Address
        self.build_expression(obj, &obj_type_str, function, writer)?;
        if offset > 0 {
            writer.write_line(&format!("i32.const {}", offset));
            writer.write_line("i32.add");
        }
        writer.write_line("local.set $scratch_addr");

        self.emit_store_with_refcount(&field_type_str, expression, function, writer)?;
        WasmGenerator::emit_store(&field_type_str, writer)?;
        Ok(())
    }

    /// Given a target address already in `$scratch_addr`, evaluates `expression` and leaves
    /// `[address, value]` on the stack ready for a store. For reference types it retains the new
    /// value and releases the value previously stored at the address.
    fn emit_store_with_refcount(
        &mut self,
        type_str: &str,
        expression: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if self.is_reference_type(type_str) {
            // An owned value already carries the reference the field/element takes over; only a
            // borrowed value needs an extra retain.
            let owns_ref = self.stores_owned_ref(expression, type_str, function)?;
            self.build_expression(expression, &type_str.to_string(), function, writer)?;
            writer.write_line("local.set $scratch_ptr");

            if !owns_ref {
                writer.write_line("local.get $scratch_ptr");
                writer.write_line("call $retain");
            }

            writer.write_line("local.get $scratch_addr");
            writer.write_line("i32.load");
            self.emit_release(type_str, writer);

            writer.write_line("local.get $scratch_addr");
            writer.write_line("local.get $scratch_ptr");
        } else {
            writer.write_line("local.get $scratch_addr");
            self.build_expression(expression, &type_str.to_string(), function, writer)?;
        }
        Ok(())
    }

    /// Builds a return statement
    pub fn build_return(
        &mut self,
        expression: &Option<ExpressionNode<'a>>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        // Inside an async poll body a `return` completes the task's `Future` and yields `Pending`
        // (the poll's wasm result is unused). Saved locals are intentionally not released in v1.
        if let Some(self_local) = self.ctx.current_async_self.clone() {
            writer.write_line(&format!("local.get ${}", self_local));
            if let Some(expr) = expression {
                let ret_type_str = function
                    .return_type
                    .as_ref()
                    .map(|t| t.get_type())
                    .unwrap_or_else(|| "int".to_string());
                self.build_expression(expr, &ret_type_str, function, writer)?;
            } else {
                writer.write_line("i32.const 0");
            }
            writer.write_line("call $dream_complete");
            writer.write_line("i32.const 0");
            writer.write_line("return");
            return Ok(());
        }
        if let Some(expr) = expression {
            let return_type = function.return_type.as_ref().ok_or_else(|| {
                Error::other(format!(
                    "function '{}' returns a value but has no declared return type",
                    function.name.text
                ))
            })?;
            let ret_type_str = return_type.get_type();
            // A primitive returned as `object` is boxed (owned); otherwise consult the classifier.
            let owns_ref = self.stores_owned_ref(expr, &ret_type_str, function)?;
            self.build_expression(expr, &ret_type_str, function, writer)?;

            // Stash the return value in a scratch local so locals can be released before we hand
            // it back. A borrowed value is retained so it survives the scope-exit releases; an
            // owned value already carries the +1 the caller will take over.
            if self.is_reference_type(strip_nullable(&ret_type_str)) {
                writer.write_line("local.set $scratch_ptr");
                if !owns_ref {
                    writer.write_line("local.get $scratch_ptr");
                    writer.write_line("call $retain");
                }
            } else if ret_type_str == "double" {
                writer.write_line("local.set $scratch_double");
            } else if ret_type_str == "float" {
                writer.write_line("local.set $scratch_float");
            } else {
                writer.write_line("local.set $scratch_ptr");
            }
        }

        // Release all local reference variables in the current function scope.
        let func_name = self
            .ctx
            .current_mangled_name
            .clone()
            .unwrap_or_else(|| function.name.text.clone());
        self.emit_release_locals(&func_name, writer);

        if expression.is_some() {
            let return_type = function.return_type.as_ref().ok_or_else(|| {
                Error::other(format!(
                    "function '{}' returns a value but has no declared return type",
                    function.name.text
                ))
            })?;
            if return_type.get_type() == "double" {
                writer.write_line("local.get $scratch_double");
            } else if return_type.get_type() == "float" {
                writer.write_line("local.get $scratch_float");
            } else {
                writer.write_line("local.get $scratch_ptr");
            }
        }

        writer.write_line("return");
        Ok(())
    }

    /// Builds a while loop
    pub fn build_while(
        &mut self,
        condition: &ExpressionNode<'a>,
        body: &[StatementNode<'a>],
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let loop_id = self.ctx.loop_counter;
        self.ctx.loop_counter += 1;
        let label = self.ctx.pending_loop_label.take();
        self.ctx.loop_stack.push((loop_id, label));

        writer.write_line(&format!("(block $loop_end_{}", loop_id));
        writer.indent();
        writer.write_line(&format!("(loop $loop_start_{}", loop_id));
        writer.indent();
        self.build_expression(condition, &"int".to_string(), function, writer)?;
        writer.write_line("i32.const 0");
        writer.write_line("i32.eq");
        writer.write_line(&format!("br_if $loop_end_{}", loop_id));

        writer.write_line(&format!("(block $continue_target_{}", loop_id));
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");

        writer.write_line(&format!("br $loop_start_{}", loop_id));
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        self.ctx.loop_stack.pop();
        Ok(())
    }

    /// Builds a do-while loop. The body runs once before the condition is tested. The condition is
    /// checked at the end of the loop; `continue` jumps to that check.
    pub fn build_do_while(
        &mut self,
        body: &[StatementNode<'a>],
        condition: &ExpressionNode<'a>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let loop_id = self.ctx.loop_counter;
        self.ctx.loop_counter += 1;
        let label = self.ctx.pending_loop_label.take();
        self.ctx.loop_stack.push((loop_id, label));

        writer.write_line(&format!("(block $loop_end_{}", loop_id));
        writer.indent();
        writer.write_line(&format!("(loop $loop_start_{}", loop_id));
        writer.indent();

        // Body runs first; `continue` targets the condition check below.
        writer.write_line(&format!("(block $continue_target_{}", loop_id));
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");

        // Re-enter the loop while the condition holds.
        self.build_expression(condition, &"int".to_string(), function, writer)?;
        writer.write_line(&format!("br_if $loop_start_{}", loop_id));

        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        self.ctx.loop_stack.pop();
        Ok(())
    }

    /// Builds a for loop
    pub fn build_for(
        &mut self,
        init: &Option<&'a StatementNode<'a>>,
        condition: &Option<ExpressionNode<'a>>,
        increment: &Option<&'a StatementNode<'a>>,
        body: &[StatementNode<'a>],
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if let Some(init_stmt) = init {
            self.build_statement(init_stmt, function, writer)?;
        }

        let loop_id = self.ctx.loop_counter;
        self.ctx.loop_counter += 1;
        let label = self.ctx.pending_loop_label.take();
        self.ctx.loop_stack.push((loop_id, label));

        writer.write_line(&format!("(block $loop_end_{}", loop_id));
        writer.indent();
        writer.write_line(&format!("(loop $loop_start_{}", loop_id));
        writer.indent();

        if let Some(cond_expr) = condition {
            self.build_expression(cond_expr, &"int".to_string(), function, writer)?;
            writer.write_line("i32.const 0");
            writer.write_line("i32.eq");
            writer.write_line(&format!("br_if $loop_end_{}", loop_id));
        }

        writer.write_line(&format!("(block $continue_target_{}", loop_id));
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");

        if let Some(inc_stmt) = increment {
            self.build_statement(inc_stmt, function, writer)?;
        }

        writer.write_line(&format!("br $loop_start_{}", loop_id));
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        self.ctx.loop_stack.pop();
        Ok(())
    }

    /// Builds a for-each loop over an array. The iterable is evaluated once into a synthetic array
    /// temp (retained for the loop's lifetime); an integer index walks `0..len`, loading each
    /// element into the user's loop variable. Lowers to the same loop scaffolding as `for`/`while`
    /// so `break`/`continue` work.
    #[allow(clippy::too_many_arguments)]
    pub fn build_foreach(
        &mut self,
        element: &SyntaxToken,
        iterable: &ExpressionNode<'a>,
        index_name: &str,
        array_name: &str,
        body: &[StatementNode<'a>],
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let array_type = self.infer_expression_type(iterable, function)?;
        let element_type = if array_type.ends_with("[]") {
            array_type[..array_type.len() - 2].to_string()
        } else {
            "int".to_string()
        };
        let element_size = WasmGenerator::element_size_of(&element_type);

        // Evaluate the iterable once into the array temp, retaining it for the loop's lifetime.
        self.build_expression(iterable, &array_type, function, writer)?;
        if self.is_reference_type(&array_type) {
            writer.write_line("local.set $scratch_ptr");
            writer.write_line("local.get $scratch_ptr");
            writer.write_line("call $retain");
            writer.write_line(&format!("local.get ${}", array_name));
            self.emit_release(&array_type, writer);
            writer.write_line("local.get $scratch_ptr");
        }
        writer.write_line(&format!("local.set ${}", array_name));

        // index = 0
        writer.write_line("i32.const 0");
        writer.write_line(&format!("local.set ${}", index_name));

        let loop_id = self.ctx.loop_counter;
        self.ctx.loop_counter += 1;
        let label = self.ctx.pending_loop_label.take();
        self.ctx.loop_stack.push((loop_id, label));

        writer.write_line(&format!("(block $loop_end_{}", loop_id));
        writer.indent();
        writer.write_line(&format!("(loop $loop_start_{}", loop_id));
        writer.indent();

        // Loop condition: index < len(array)  (exit when not less-than)
        writer.write_line(&format!("local.get ${}", index_name));
        writer.write_line(&format!("local.get ${}", array_name));
        writer.write_line("i32.load");
        writer.write_line("i32.lt_s");
        writer.write_line("i32.eqz");
        writer.write_line(&format!("br_if $loop_end_{}", loop_id));

        // element = array[index]
        writer.write_line(&format!("local.get ${}", array_name));
        writer.write_line("i32.const 4");
        writer.write_line("i32.add");
        writer.write_line(&format!("local.get ${}", index_name));
        if element_size != 1 {
            writer.write_line(&format!("i32.const {}", element_size));
            writer.write_line("i32.mul");
        }
        writer.write_line("i32.add");
        WasmGenerator::emit_load(&element_type, writer)?;
        if self.is_reference_type(&element_type) {
            writer.write_line("local.set $scratch_ptr");
            writer.write_line("local.get $scratch_ptr");
            writer.write_line("call $retain");
            writer.write_line(&format!("local.get ${}", element.text));
            self.emit_release(&element_type, writer);
            writer.write_line("local.get $scratch_ptr");
        }
        writer.write_line(&format!("local.set ${}", element.text));

        // Body, wrapped so `continue` targets the index increment.
        writer.write_line(&format!("(block $continue_target_{}", loop_id));
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");

        // index = index + 1
        writer.write_line(&format!("local.get ${}", index_name));
        writer.write_line("i32.const 1");
        writer.write_line("i32.add");
        writer.write_line(&format!("local.set ${}", index_name));

        writer.write_line(&format!("br $loop_start_{}", loop_id));
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");

        self.ctx.loop_stack.pop();
        Ok(())
    }

    /// Resolves a break/continue target loop id. With a label, finds the nearest enclosing loop
    /// carrying that label; otherwise the innermost loop.
    fn resolve_loop_id(&self, label: &Option<String>) -> Option<usize> {
        match label {
            Some(name) => self
                .ctx
                .loop_stack
                .iter()
                .rev()
                .find(|(_, l)| l.as_deref() == Some(name.as_str()))
                .map(|(id, _)| *id),
            None => self.ctx.loop_stack.last().map(|(id, _)| *id),
        }
    }

    /// Builds a break statement, optionally targeting a labeled loop.
    pub fn build_break(
        &mut self,
        label: &Option<String>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if let Some(loop_id) = self.resolve_loop_id(label) {
            writer.write_line(&format!("br $loop_end_{}", loop_id));
        } else {
            writer.write_line("br 1"); // Fallback, though semantic analyzer should catch this
        }
        Ok(())
    }

    /// Builds a continue statement, optionally targeting a labeled loop.
    pub fn build_continue(
        &mut self,
        label: &Option<String>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if let Some(loop_id) = self.resolve_loop_id(label) {
            writer.write_line(&format!("br $continue_target_{}", loop_id));
        } else {
            writer.write_line("br 0"); // Fallback
        }
        Ok(())
    }

    /// Builds a switch statement. The subject is evaluated once into `$scratch_switch`, then the
    /// cases are lowered to a nested `if/else` chain comparing the subject to each label. There is
    /// no implicit fallthrough: matching a case runs only its body. Once a body is entered the
    /// subject is never read again, so a single scratch local is safe even for nested switches.
    pub fn build_switch(
        &mut self,
        subject: &ExpressionNode<'a>,
        cases: &Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
        default_body: &Option<&'a [StatementNode<'a>]>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let subject_type = self.infer_expression_type(subject, function)?;
        self.build_expression(subject, &subject_type, function, writer)?;
        writer.write_line("local.set $scratch_switch");
        self.build_switch_cases(&subject_type, cases, default_body, 0, function, writer)
    }

    /// Recursively emits the nested `if/else` chain for switch cases starting at `index`.
    fn build_switch_cases(
        &mut self,
        subject_type: &str,
        cases: &Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
        default_body: &Option<&'a [StatementNode<'a>]>,
        index: usize,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if index == cases.len() {
            if let Some(db) = default_body {
                self.build_body(db, function, writer)?;
            }
            return Ok(());
        }

        let (labels, body) = &cases[index];
        let is_string = strip_nullable(subject_type) == "string";
        // condition = (subject == label0) || (subject == label1) || ...
        for (i, label) in labels.iter().enumerate() {
            writer.write_line("local.get $scratch_switch");
            self.build_expression(label, &subject_type.to_string(), function, writer)?;
            if is_string {
                writer.write_line("call $string_eq");
            } else {
                writer.write_line("i32.eq");
            }
            if i > 0 {
                writer.write_line("i32.or");
            }
        }

        writer.write_line("(if");
        writer.indent();
        writer.write_line("(then");
        writer.indent();
        self.build_body(body, function, writer)?;
        writer.unindent();
        writer.write_line(")");
        writer.write_line("(else");
        writer.indent();
        self.build_switch_cases(
            subject_type,
            cases,
            default_body,
            index + 1,
            function,
            writer,
        )?;
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Builds an if-else statement
    pub fn build_if_else(
        &mut self,
        condition: &ExpressionNode<'a>,
        body: &'a [StatementNode<'a>],
        else_if: &Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>,
        else_body: &Option<&'a [StatementNode<'a>]>,
        function: &FunctionNode<'a>,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        let mut arr: Vec<(Option<&ExpressionNode<'a>>, &'a [StatementNode<'a>])> = Vec::new();
        arr.push((Some(condition), body));
        for i in else_if.iter() {
            arr.push((Some(&i.0), i.1));
        }
        if let Some(eb) = else_body {
            arr.push((None, eb));
        }
        self.build_if_else_parts(&arr, function, 0, writer)?;
        Ok(())
    }

    /// Recursively builds the parts of an if-else chain
    pub fn build_if_else_parts(
        &mut self,
        parts: &Vec<(Option<&ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
        function: &FunctionNode<'a>,
        index: usize,
        writer: &mut IndentedTextWriter,
    ) -> Result<(), Error> {
        if index == parts.len() {
            return Ok(());
        }
        let cur = &parts[index];
        if cur.0.is_none() && index == parts.len() - 1 {
            self.build_body(cur.1, function, writer)?;
        } else {
            let mut is_constant_true = false;
            let mut is_constant_false = false;
            if let Some(ExpressionNode::IsExpression(left, right_type)) = cur.0 {
                let left_type = self.infer_expression_type(left, function)?;
                // `is` on an `object` is a runtime tag check, not a compile-time constant.
                if strip_nullable(&left_type) != "object" {
                    if left_type == right_type.get_type() {
                        is_constant_true = true;
                    } else {
                        is_constant_false = true;
                    }
                }
            }

            if is_constant_true {
                self.build_body(cur.1, function, writer)?;
                return Ok(());
            } else if is_constant_false {
                self.build_if_else_parts(parts, function, index + 1, writer)?;
                return Ok(());
            }

            self.build_expression(cur.0.unwrap(), &"int".to_string(), function, writer)?;
            writer.write_line("(if");
            writer.indent();
            writer.write_line("(then");
            writer.indent();
            self.build_body(cur.1, function, writer)?;
            writer.unindent();
            writer.write_line(")");
            if index + 1 < parts.len() {
                writer.write_line("(else");
                writer.indent();
                self.build_if_else_parts(parts, function, index + 1, writer)?;
                writer.unindent();
                writer.write_line(")");
            }
            writer.unindent();
            writer.write_line(")");
        }
        Ok(())
    }
}
