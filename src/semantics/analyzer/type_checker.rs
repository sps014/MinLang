use bumpalo::Bump;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, Type, ProgramNode, StatementNode};
use crate::syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use crate::syntax::nodes::function::ParameterNode;
use crate::syntax::nodes::types::{canonical_type_name, mangle_generic, mangle_with_suffixes, strip_array, strip_nullable};
use crate::syntax::syntax_tree::SyntaxTree;
use crate::syntax::text::line_text::LineText;
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::semantics::function_control_flow::FunctionControlGraph;
use crate::semantics::function_table::{FunctionTable, FunctionTableInfo, OverloadResolution, overload_arg_compatible};
use crate::semantics::symbol_table::SymbolTable;
use crate::semantics::struct_table::StructTable;
use crate::driver::diagnostics::DiagnosticBag;
use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_function(&mut self,function:&FunctionNode<'a>, diagnostics: &mut DiagnosticBag) -> Result<Rc<RefCell<SymbolTable>>, ()> {
        let param_table=Rc::new(RefCell::new(self.add_function_param_table(function, diagnostics)?));
        self.current_function_is_async = function.is_async;
        self.analyze_body(function.body,function,Some(&param_table),false, diagnostics)?;
        // Enforce the v1 `await` placement rules (only in async functions, only at statement
        // position) and that non-async functions contain no `await` at all.
        self.check_await_positions(function, diagnostics);
        self.current_function_is_async = false;
        // check return
        let mut graph=FunctionControlGraph::new(function);
        if let Err(e) = graph.build() {
            diagnostics.report_error(e.to_string(), Some(function.name.position.clone()));
        }
        Ok(param_table.clone())
    }

    /// Validates the v1 `await` placement rules. `await` may only appear inside an `async fun`,
    /// and only as a whole top-level statement: `await e;`, `let x = await e;`, or `return await e;`.
    /// Awaits nested in sub-expressions, loops, branches, or non-async functions are rejected.
    pub(super) fn check_await_positions(&self, function: &FunctionNode<'a>, diagnostics: &mut DiagnosticBag) {
        if !function.is_async {
            for stmt in function.body.iter() {
                self.forbid_await_in_stmt(stmt, "'await' can only be used inside an 'async' function", diagnostics);
            }
            return;
        }
        for stmt in function.body.iter() {
            match stmt {
                StatementNode::Declaration(_, _, ExpressionNode::Await(inner), _) => {
                    self.forbid_await_in_expr(inner, diagnostics);
                }
                StatementNode::Return(Some(ExpressionNode::Await(inner))) => {
                    self.forbid_await_in_expr(inner, diagnostics);
                }
                StatementNode::AwaitStmt(inner) => {
                    self.forbid_await_in_expr(inner, diagnostics);
                }
                other => self.forbid_await_in_stmt(other,
                    "'await' must appear as a top-level statement (e.g. `let x = await e;` or `await e;`); awaiting inside loops, branches, or sub-expressions is not supported yet",
                    diagnostics),
            }
        }
    }

    /// Reports `message` at every `await` found anywhere inside `stmt` (including nested bodies).
    fn forbid_await_in_stmt(&self, stmt: &StatementNode<'a>, message: &str, diagnostics: &mut DiagnosticBag) {
        match stmt {
            StatementNode::AwaitStmt(inner) => {
                diagnostics.report_error(message.to_string(), inner.position());
                self.scan_expr_await(inner, message, diagnostics);
            }
            StatementNode::Declaration(_, _, e, _) | StatementNode::Assignment(_, e)
            | StatementNode::IndexAssignment(_, _, e) | StatementNode::MemberAssignment(_, _, e) => {
                self.scan_expr_await(e, message, diagnostics);
            }
            StatementNode::Return(Some(e)) => self.scan_expr_await(e, message, diagnostics),
            StatementNode::FunctionInvocation(_, _, args) => {
                for a in args { self.scan_expr_await(a, message, diagnostics); }
            }
            StatementNode::MethodInvocation(_, _, _, args) => {
                for a in args { self.scan_expr_await(a, message, diagnostics); }
            }
            StatementNode::IfElse(c, b, elifs, eb) => {
                self.scan_expr_await(c, message, diagnostics);
                for s in b.iter() { self.forbid_await_in_stmt(s, message, diagnostics); }
                for (ec, eb2) in elifs.iter() {
                    self.scan_expr_await(ec, message, diagnostics);
                    for s in eb2.iter() { self.forbid_await_in_stmt(s, message, diagnostics); }
                }
                if let Some(eb) = eb { for s in eb.iter() { self.forbid_await_in_stmt(s, message, diagnostics); } }
            }
            StatementNode::While(c, b) | StatementNode::DoWhile(b, c) => {
                self.scan_expr_await(c, message, diagnostics);
                for s in b.iter() { self.forbid_await_in_stmt(s, message, diagnostics); }
            }
            StatementNode::For(init, cond, inc, body) => {
                if let Some(i) = init { self.forbid_await_in_stmt(i, message, diagnostics); }
                if let Some(c) = cond { self.scan_expr_await(c, message, diagnostics); }
                if let Some(i) = inc { self.forbid_await_in_stmt(i, message, diagnostics); }
                for s in body.iter() { self.forbid_await_in_stmt(s, message, diagnostics); }
            }
            StatementNode::ForEach(_, iterable, _, _, body) => {
                self.scan_expr_await(iterable, message, diagnostics);
                for s in body.iter() { self.forbid_await_in_stmt(s, message, diagnostics); }
            }
            StatementNode::Switch(subject, cases, default_body) => {
                self.scan_expr_await(subject, message, diagnostics);
                for (_, body) in cases.iter() { for s in body.iter() { self.forbid_await_in_stmt(s, message, diagnostics); } }
                if let Some(db) = default_body { for s in db.iter() { self.forbid_await_in_stmt(s, message, diagnostics); } }
            }
            StatementNode::Labeled(_, inner) => self.forbid_await_in_stmt(inner, message, diagnostics),
            _ => {}
        }
    }

    /// Reports `message` if `expr` contains any `await` (used to forbid awaits in sub-expressions).
    fn forbid_await_in_expr(&self, expr: &ExpressionNode<'a>, diagnostics: &mut DiagnosticBag) {
        self.scan_expr_await(expr,
            "'await' cannot appear inside another expression; bind it first (e.g. `let x = await e;`)",
            diagnostics);
    }

    /// Recursively reports `message` at every nested `await` expression within `expr`.
    fn scan_expr_await(&self, expr: &ExpressionNode<'a>, message: &str, diagnostics: &mut DiagnosticBag) {
        match expr {
            ExpressionNode::Await(inner) => {
                diagnostics.report_error(message.to_string(), inner.position());
                self.scan_expr_await(inner, message, diagnostics);
            }
            ExpressionNode::Binary(l, _, r) => { self.scan_expr_await(l, message, diagnostics); self.scan_expr_await(r, message, diagnostics); }
            ExpressionNode::Unary(_, e) | ExpressionNode::Parenthesized(e) | ExpressionNode::Cast(_, e)
            | ExpressionNode::IsExpression(e, _) => self.scan_expr_await(e, message, diagnostics),
            ExpressionNode::FunctionCall(_, _, args) => { for a in args { self.scan_expr_await(a, message, diagnostics); } }
            ExpressionNode::MethodCall(obj, _, _, args) => { self.scan_expr_await(obj, message, diagnostics); for a in args { self.scan_expr_await(a, message, diagnostics); } }
            ExpressionNode::ArrayLiteral(elems) => { for e in elems { self.scan_expr_await(e, message, diagnostics); } }
            ExpressionNode::IndexAccess(a, i) => { self.scan_expr_await(a, message, diagnostics); self.scan_expr_await(i, message, diagnostics); }
            ExpressionNode::MemberAccess(o, _) => self.scan_expr_await(o, message, diagnostics),
            ExpressionNode::StructInstantiation(_, _, fields) => { for (_, e) in fields { self.scan_expr_await(e, message, diagnostics); } }
            ExpressionNode::Ternary(c, t, e) => { self.scan_expr_await(c, message, diagnostics); self.scan_expr_await(t, message, diagnostics); self.scan_expr_await(e, message, diagnostics); }
            _ => {}
        }
    }
    pub(super) fn add_function_param_table(&mut self,function:&FunctionNode<'a>, diagnostics: &mut DiagnosticBag) -> Result<SymbolTable, ()> {
        let mut param_table=SymbolTable::new(None);
        for param in function.parameters.iter() {
            self.check_reserved_name(&param.name, "parameter", diagnostics);
            if let Err(e) = param_table.add_symbol(param.name.text.clone(), param.type_.clone()) {
                diagnostics.report_error(e.to_string(), Some(param.name.position.clone()));
            }
        }
        Ok(param_table)
    }

    pub(super) fn analyze_body(&mut self, body:&[StatementNode<'a>], parent_function:&FunctionNode<'a>,
                    parent_table:Option<&Rc<RefCell<SymbolTable>>>,has_parent_loop:bool, diagnostics: &mut DiagnosticBag) ->Result<(),()> {

        let parent_scope =match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope.clone())));
        if parent_scope.is_some()
        {
            let parent_table=&parent_scope.unwrap();
            (*parent_table).borrow_mut().add_child(symbol_table.clone());
        }
        for statement in body.iter() {
            let clone=&symbol_table.clone();
            self.analyze_statement(statement,parent_function,&clone,has_parent_loop, diagnostics)?;
        }
        Ok(())
    }
    pub(super) fn analyze_statement(&mut self,statement:&StatementNode<'a>,parent_function:&FunctionNode<'a>,
                         symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        match statement
        {
            StatementNode::Declaration(left, type_annotation, right, is_const) =>
                self.analyze_declaration(left, type_annotation, right, *is_const, parent_function,&symbol_table, diagnostics)?,
            StatementNode::Assignment(left,right) =>
                self.analyze_assignment(left,right,parent_function,&symbol_table, diagnostics)?,
            StatementNode::IndexAssignment(left, index, right) =>
                self.analyze_index_assignment(left, index, right, parent_function, &symbol_table, diagnostics)?,
            StatementNode::MemberAssignment(obj, member, right) =>
                self.analyze_member_assignment(obj, member, right, parent_function, &symbol_table, diagnostics)?,
            StatementNode::IfElse(condition,if_body,
                                  else_if,else_body)=>
                self.analyze_if_else(condition,if_body,
                                     else_if,else_body,parent_function,&symbol_table,has_parent_while, diagnostics)?,
            StatementNode::Return(expression) =>
                self.analyze_return(expression,parent_function,&symbol_table, diagnostics)?,
            StatementNode::While(condition,body) =>
                self.analyze_while(condition,body,parent_function,&symbol_table, diagnostics)?,
            StatementNode::DoWhile(body,condition) =>
                self.analyze_while(condition,body,parent_function,&symbol_table, diagnostics)?,
            StatementNode::For(init,condition,increment,body) =>
                self.analyze_for(init,condition,increment,body,parent_function,&symbol_table, diagnostics)?,
            StatementNode::ForEach(element, iterable, index_name, array_name, body) =>
                self.analyze_foreach(element, iterable, index_name, array_name, body, parent_function, &symbol_table, diagnostics)?,
            StatementNode::Switch(subject, cases, default_body) =>
                self.analyze_switch(subject, cases, default_body, parent_function, &symbol_table, has_parent_while, diagnostics)?,
            StatementNode::Labeled(label, inner) => {
                self.loop_labels.push(label.clone());
                let result = self.analyze_statement(inner, parent_function, symbol_table, has_parent_while, diagnostics);
                self.loop_labels.pop();
                result?;
            },
            StatementNode::Break(label)=>
                self.analyze_break(label,parent_function,has_parent_while, diagnostics)?,
            StatementNode::Continue(label)=>
                self.analyze_continue(label,parent_function,has_parent_while, diagnostics)?,
            StatementNode::FunctionInvocation(name, generic_args, params) =>
                {self.analyze_function_call(name, generic_args, params,parent_function,symbol_table, diagnostics)?;},
            StatementNode::MethodInvocation(obj, method, generic_args, params) =>
                {self.analyze_method_call(obj, method, generic_args, params, parent_function, symbol_table, diagnostics)?;},
            StatementNode::AwaitStmt(future_expr) => {
                let fut = self.analyze_expression(future_expr, parent_function, symbol_table, diagnostics)?;
                if Self::future_inner_type(&fut).is_none() {
                    diagnostics.report_error(
                        format!("'await' expects a Future value, got {}", fut.get_type()),
                        future_expr.position(),
                    );
                }
            },
        };
        Ok(())
    }
    /// Resolves an overloaded base name against the concrete `arg_types`, returning the selected
    /// signature or a human-readable error (no match / ambiguous). Used by both free-function and
    /// method call analysis (methods prepend the receiver type as the implicit `this` argument).
    pub(super) fn select_function_overload(&self, base: &str, arg_types: &[String]) -> Result<FunctionTableInfo, String> {
        let compat = |param: &str, arg: &str| overload_arg_compatible(param, arg, |t| self.enum_table.contains_key(t));
        match self.function_table.select_overload(base, arg_types, compat) {
            OverloadResolution::Unique(key) => Ok(self.function_table.get_function(&key).unwrap()),
            OverloadResolution::None => Err(format!(
                "No overload of '{}' matches argument types ({})", base, arg_types.join(", ")
            )),
            OverloadResolution::Ambiguous(keys) => Err(format!(
                "Ambiguous call to '{}' with argument types ({}); candidates: {}", base, arg_types.join(", "), keys.join(", ")
            )),
        }
    }

    /// Analyzes a static-method call `Type.method(args)` (resolved by the caller to the type
    /// `type_name`). Static methods have no implicit `this`, so the explicit arguments map 1:1 to
    /// the declared parameters.
    pub(super) fn analyze_static_call(&mut self, type_name: &str, method: &SyntaxToken, params: &Vec<ExpressionNode<'a>>,
                                      parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>,
                                      diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        let base = format!("{}_{}", type_name, method.text);

        let mut arg_types = Vec::new();
        for param in params.iter() {
            arg_types.push(self.analyze_expression(param, parent_function, symbol_table, diagnostics)?.get_type());
        }

        let store_sig = if self.function_table.is_overloaded(&base) {
            match self.select_function_overload(&base, &arg_types) {
                Ok(sig) => sig,
                Err(message) => {
                    diagnostics.report_error(message, Some(method.position.clone()));
                    return Ok(Type::Void);
                }
            }
        } else {
            match self.function_table.get_function(&base) {
                Ok(s) => s.clone(),
                Err(_) => {
                    diagnostics.report_error(
                        format!("Type '{}' has no static method '{}'", type_name, method.text),
                        Some(method.position.clone()),
                    );
                    return Ok(Type::Void);
                }
            }
        };

        if method.text.starts_with('_') && !self.in_methods_of(parent_function, type_name) {
            diagnostics.report_error(
                format!("'{}' is private to '{}'", method.text, type_name),
                Some(method.position.clone()),
            );
        }

        let expected_params = store_sig.parameters.clone();
        if expected_params.len() != arg_types.len() {
            diagnostics.report_error(
                format!("static method {} expects {} parameters, got {}", base, expected_params.len(), arg_types.len()),
                Some(method.position.clone()),
            );
            return Ok(store_sig.return_type.unwrap_or(Type::Void));
        }
        for (i, given_type) in arg_types.iter().enumerate() {
            let expected = &expected_params[i];
            if expected == "object" {
                continue;
            }
            let numeric_ok = matches!(
                (expected.as_str(), given_type.as_str()),
                ("int", "float") | ("float", "int") | ("double", "int") | ("int", "double") | ("float", "double") | ("double", "float")
            );
            if numeric_ok {
                continue;
            }
            if given_type != expected {
                diagnostics.report_error(
                    format!("static method {} expects parameter {} to be {}, got {}", base, i + 1, expected, given_type),
                    Some(method.position.clone()),
                );
            }
        }

        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }

    /// True when `parent_function` is a method whose implicit `this` receiver has base type
    /// `base_name` (allowing for monomorphized generic variants). Used to gate access to
    /// `_`-prefixed (private) members.
    pub(super) fn in_methods_of(&self, parent_function: &FunctionNode<'a>, base_name: &str) -> bool {
        let Some(first) = parent_function.parameters.first() else { return false; };
        if first.name.text != "this" {
            return false;
        }
        let this_base = Self::resolve_struct_parts(&first.type_)
            .map(|(b, _)| b)
            .unwrap_or_else(|| strip_nullable(&first.type_.get_type()).to_string());
        this_base == base_name
            || this_base.starts_with(&format!("{}_", base_name))
            || base_name.starts_with(&format!("{}_", this_base))
    }

    pub(super) fn analyze_function_call(&mut self,name:&SyntaxToken,generic_args: &Option<Vec<Type>>,params:&Vec<ExpressionNode<'a>>,
                                   parent_function:&FunctionNode<'a>,
                                   symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        // Object-protocol builtins: accept exactly one argument of any type.
        if matches!(name.text.as_str(), "print" | "println" | "to_string" | "hash_code") {
            if params.len() != 1 {
                diagnostics.report_error(
                    format!("'{}' expects exactly 1 argument, got {}", name.text, params.len()),
                    Some(name.position.clone()),
                );
            }
            for param in params.iter() {
                self.analyze_expression(param, parent_function, symbol_table, diagnostics)?;
            }
            return Ok(match name.text.as_str() {
                "to_string" => Type::String(synthetic_token(TokenKind::DataTypeToken, "string")),
                "hash_code" => Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int")),
                _ => Type::Void,
            });
        }

        // Async intrinsics: `sleep`, `all`, `any`, `race`. These are compiler-known (not in the
        // function table) because their signatures are generic over `Future<T>`.
        if matches!(name.text.as_str(), "sleep" | "all" | "any" | "race") {
            return self.analyze_async_intrinsic(name, params, parent_function, symbol_table, diagnostics);
        }

        // `array_new<T>(n)`: allocates a fresh, zero-initialized array of `n` elements of type `T`.
        // The element type is read from the explicit type argument (resolved through the active
        // monomorphization bindings so `array_new<T>` inside a `List<int>` method yields `int[]`).
        if name.text == "array_new" {
            let element = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error("'array_new' requires a type argument, e.g. array_new<int>(n)".to_string(), Some(name.position.clone()));
                    Type::Void
                }
            };
            if params.len() != 1 {
                diagnostics.report_error(format!("'array_new' expects exactly 1 argument (length), got {}", params.len()), Some(name.position.clone()));
            }
            for param in params.iter() {
                let pt = self.analyze_expression(param, parent_function, symbol_table, diagnostics)?;
                if pt.get_type() != "int" {
                    diagnostics.report_error(format!("'array_new' length must be int, got {}", pt.get_type()), param.position());
                }
            }
            return Ok(Type::Array(Box::new(element)));
        }

        let mut function_name=name.text.clone();
        let mut params_types=vec![];
        for param in params.iter() {
            params_types.push(self.analyze_expression(param,parent_function,symbol_table, diagnostics)?.get_type());
        }

        // Indirect call: if the called name is a local variable of function type, validate the
        // arguments against the function-type signature and return its result type.
        if let Ok(Type::Function(param_types, ret)) = (*symbol_table).as_ref().borrow().get_symbol(name) {
            if param_types.len() != params_types.len() {
                diagnostics.report_error(
                    format!("function value '{}' expects {} arguments, got {}", name.text, param_types.len(), params_types.len()),
                    Some(name.position.clone()),
                );
                return Ok((*ret).clone());
            }
            for i in 0..param_types.len() {
                let expected = param_types[i].get_type();
                if expected != "object" && expected != params_types[i] {
                    diagnostics.report_error(
                        format!("function value '{}' expects argument {} to be {}, got {}", name.text, i + 1, expected, params_types[i]),
                        Some(name.position.clone()),
                    );
                }
            }
            return Ok((*ret).clone());
        }

        // Constructor call: `Struct(args)` / `Struct<T>(args)`. Only treated as a constructor
        // when no free function (concrete or generic) shadows the name, so prelude factory
        // functions such as `List<T>()` keep their behaviour.
        if self.function_table.get_function(&function_name).is_err()
            && !self.function_table.is_overloaded(&function_name)
            && !self.generic_functions.contains_key(&function_name)
            && (self.struct_table.get_struct(&function_name).is_some()
                || self.generic_structs.contains_key(&function_name)) {
            return self.analyze_constructor_call(name, generic_args, &params_types, diagnostics);
        }

        // Monomorphization: bind every generic parameter to a concrete type, then register
        // (once) a specialized signature under the mangled name.
        if self.generic_functions.contains_key(&function_name) {
            let template = *self.generic_functions.get(&function_name).unwrap();
            let bindings = self.infer_generic_bindings(template, generic_args, &params_types, &name.position, diagnostics);
            let mangled_name = mangle_bindings(&function_name, &bindings);

            if self.function_table.get_function(&mangled_name).is_err() {
                // Store a clone with its signature monomorphized (params + return type made
                // concrete), mirroring how struct methods are specialized. The body is shared and
                // resolved against the bindings during analysis/codegen, so the declared return
                // type (e.g. `List<T>` -> `List_int`) stays consistent with what the body builds.
                let mut specialized = template.clone();
                Self::substitute_generic_signature(&mut specialized, &bindings);
                let specialized_ref: &'a FunctionNode<'a> = self.arena.alloc(specialized);
                self.instantiated_generics.insert(mangled_name.clone(), (bindings.clone(), specialized_ref));

                let info = FunctionTableInfo {
                    name: mangled_name.clone(),
                    parameters: template.parameters.iter()
                        .map(|p| Self::monomorphize_type(&p.type_, &bindings).get_type())
                        .collect(),
                    return_type: template.return_type.as_ref()
                        .map(|ret| Self::monomorphize_type(ret, &bindings)),
                    is_async: template.is_async,
                };

                let _ = self.function_table.add_function(mangled_name.clone(), info);
            }
            function_name = mangled_name;
        }

        // Overloaded free functions resolve by argument types; non-overloaded names keep the
        // direct single-signature lookup (and its precise per-argument diagnostics below).
        let store_sig = if self.function_table.is_overloaded(&function_name) {
            match self.select_function_overload(&function_name, &params_types) {
                Ok(sig) => sig,
                Err(message) => {
                    diagnostics.report_error(message, Some(name.position.clone()));
                    return Ok(Type::Void);
                }
            }
        } else {
            match self.function_table.get_function(&function_name) {
                Ok(sig) => sig,
                Err(e) => {
                    diagnostics.report_error(e.to_string(), Some(name.position.clone()));
                    return Ok(Type::Void);
                }
            }
        };

        if store_sig.parameters.len()!=params_types.len() {
            diagnostics.report_error(format!("Function {} has {} params but {} params are given",
                                                           function_name,store_sig.parameters.len(),params_types.len()), Some(name.position.clone()));
            return Ok(Type::Void);
        }

        for i in 0..params_types.len() {
            // A parameter declared `object` accepts any argument type (boxing happens in codegen).
            if store_sig.parameters.get(i).map(|s| s == "object").unwrap_or(false) {
                continue;
            }
            if store_sig.parameters.get(i)!=params_types.get(i) {
                let expected = store_sig.parameters.get(i).map(|s| s.as_str()).unwrap_or("");
                let given = params_types.get(i).map(|s| s.as_str()).unwrap_or("");
                if self.enum_int_compatible(expected, given) {
                    continue;
                }
                diagnostics.report_error(format!("Function {} has param {} of type {:?} but param {} of type {:?} is given",
                                                               function_name,i,store_sig.parameters.get(i),i,params_types[i]), Some(name.position.clone()));
            }
        }

        //let r_type=&store_sig.return_type;
        // Calling an `async fun` is eager and yields a `Future<T>` handle (where `T` is the
        // declared return type). It is NOT auto-awaited; `await` retrieves the `T`.
        if store_sig.is_async {
            return Ok(Self::future_type(store_sig.return_type.unwrap_or(Type::Void)));
        }
        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }

    /// Types the async intrinsics: `sleep(ms: int): Future<void>`, `all(xs: Future<T>[]):
    /// Future<T[]>`, `any`/`race(xs: Future<T>[]): Future<T>`.
    pub(super) fn analyze_async_intrinsic(&mut self, name: &SyntaxToken, params: &Vec<ExpressionNode<'a>>,
                                          parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>,
                                          diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        if name.text == "sleep" {
            if params.len() != 1 {
                diagnostics.report_error(format!("'sleep' expects exactly 1 argument (milliseconds), got {}", params.len()), Some(name.position.clone()));
            }
            for p in params { let pt = self.analyze_expression(p, parent_function, symbol_table, diagnostics)?;
                if pt.get_type() != "int" {
                    diagnostics.report_error(format!("'sleep' expects an int argument, got {}", pt.get_type()), p.position());
                }
            }
            return Ok(Self::future_type(Type::Void));
        }

        // all/any/race take a single `Future<T>[]` argument.
        if params.len() != 1 {
            diagnostics.report_error(format!("'{}' expects exactly 1 argument (a Future array), got {}", name.text, params.len()), Some(name.position.clone()));
            return Ok(Self::future_type(Type::Void));
        }
        let arg_type = self.analyze_expression(&params[0], parent_function, symbol_table, diagnostics)?;
        let inner_t = match &arg_type {
            Type::Array(inner) => match Self::future_inner_type(inner) {
                Some(t) => t,
                None => {
                    diagnostics.report_error(format!("'{}' expects an array of Future values, got {}", name.text, arg_type.get_type()), params[0].position());
                    Type::Void
                }
            },
            _ => {
                diagnostics.report_error(format!("'{}' expects an array of Future values, got {}", name.text, arg_type.get_type()), params[0].position());
                Type::Void
            }
        };
        if name.text == "all" {
            // Future<T[]>
            Ok(Self::future_type(Type::Array(Box::new(inner_t))))
        } else {
            // any / race -> Future<T>
            Ok(Self::future_type(inner_t))
        }
    }

    /// Type-checks a constructor call `Struct(args)`. When the struct defines a custom `constructor`
    /// the call is checked against `init`'s parameters; otherwise it is checked positionally
    /// against the struct's fields in declaration order (the auto-generated constructor).
    pub(super) fn analyze_constructor_call(&mut self, name: &SyntaxToken, generic_args: &Option<Vec<Type>>, params_types: &[String], diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        let struct_name = match generic_args {
            Some(args) if !args.is_empty() => {
                self.ensure_struct_instantiated(&name.text, args, &name.position, diagnostics);
                mangle_generic(&name.text, args)
            }
            _ => {
                if self.generic_structs.contains_key(&name.text) {
                    diagnostics.report_error(
                        format!("Generic class '{}' requires type arguments, e.g. {}<int>(...)", name.text, name.text),
                        Some(name.position.clone()),
                    );
                }
                name.text.clone()
            }
        };

        let init_name = format!("{}_constructor", struct_name);
        let expected: Vec<String> = if let Ok(sig) = self.function_table.get_function(&init_name) {
            // `constructor` is registered as a method, so parameter 0 is the implicit `this`.
            sig.parameters.iter().skip(1).cloned().collect()
        } else if let Some(info) = self.struct_table.get_struct(&struct_name) {
            let mut ordered: Vec<(&String, &crate::semantics::struct_table::StructFieldInfo)> =
                info.fields.iter().collect();
            ordered.sort_by_key(|(_, f)| f.offset);
            ordered.iter().map(|(_, f)| f.type_.get_type()).collect()
        } else {
            Vec::new()
        };

        if expected.len() != params_types.len() {
            diagnostics.report_error(
                format!("Constructor for '{}' expects {} argument(s), but {} were given", struct_name, expected.len(), params_types.len()),
                Some(name.position.clone()),
            );
        } else {
            for i in 0..expected.len() {
                let e = expected[i].as_str();
                let g = params_types[i].as_str();
                if e == "object" || e == g || self.enum_int_compatible(e, g) {
                    continue;
                }
                diagnostics.report_error(
                    format!("Constructor for '{}' expects argument {} to be '{}', got '{}'", struct_name, i + 1, e, g),
                    Some(name.position.clone()),
                );
            }
        }

        Ok(Type::Struct(synthetic_token(TokenKind::IdentifierToken, &struct_name), None))
    }

    pub(super) fn analyze_method_call(&mut self, obj: &ExpressionNode<'a>, method: &SyntaxToken, _generic_args: &Option<Vec<Type>>, params: &Vec<ExpressionNode<'a>>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        // `Math.<fn>(...)`: the math namespace. `Math` is not a value, so intercept before
        // trying to analyze it as an expression.
        if let ExpressionNode::Identifier(id) = obj {
            if id.text == "Math" {
                return self.analyze_math_call(method, params, parent_function, symbol_table, diagnostics);
            }

            // `Type.method(args)`: a static-method call. The receiver names a type (not a local
            // variable), so resolve `{type}_{method}` directly with no implicit `this`.
            let is_local = (*symbol_table).as_ref().borrow().get_symbol(id).is_ok();
            if !is_local {
                let type_name = canonical_type_name(&id.text).unwrap_or(id.text.as_str()).to_string();
                let base = format!("{}_{}", type_name, method.text);
                if self.function_table.is_overloaded(&base) || self.function_table.get_function(&base).is_ok() {
                    return self.analyze_static_call(&type_name, method, params, parent_function, symbol_table, diagnostics);
                }
            }
        }

        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

        // Private methods (`_name`) may only be called from within the declaring type's own methods.
        if method.text.starts_with('_') {
            let receiver_base = strip_nullable(&obj_type.get_type()).to_string();
            let base_name = Self::resolve_struct_parts(&obj_type)
                .map(|(b, _)| b)
                .unwrap_or_else(|| receiver_base.clone());
            if !self.in_methods_of(parent_function, &base_name) {
                diagnostics.report_error(
                    format!("'{}' is private to '{}'", method.text, base_name),
                    Some(method.position.clone()),
                );
            }
        }

        // `EnumValue.name()`: built-in accessor returning the variant name as a string.
        if method.text == "name" {
            let base = strip_nullable(&obj_type.get_type()).to_string();
            if self.enum_table.contains_key(&base) {
                if !params.is_empty() {
                    diagnostics.report_error(format!("'name' takes no arguments, got {}", params.len()), Some(method.position.clone()));
                }
                return Ok(Type::String(synthetic_token(TokenKind::DataTypeToken, "string")));
            }
        }

        // `arr.len()` / `str.len()`: built-in length method on arrays and strings.
        if method.text == "len" {
            let base = strip_nullable(&obj_type.get_type()).to_string();
            if base.ends_with("[]") || base == "string" {
                if !params.is_empty() {
                    diagnostics.report_error(format!("'len' takes no arguments, got {}", params.len()), Some(method.position.clone()));
                }
                return Ok(Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int")));
            }
        }

        // Struct receivers are monomorphized to their concrete type name; primitive/`object`
        // receivers (which can carry methods via `extend`) use their canonical type name directly.
        let struct_name = match Self::resolve_struct_parts(&obj_type) {
            Some((base_name, generic_args)) => {
                self.ensure_struct_instantiated(&base_name, &generic_args, &method.position, diagnostics);
                mangle_generic(&base_name, &generic_args)
            }
            None => strip_nullable(&obj_type.get_type()).to_string(),
        };

        let mangled_name = format!("{}_{}", struct_name, method.text);

        // Analyze the explicit arguments once, then resolve the method (overloaded methods select
        // by argument types, with the receiver supplied as the implicit `this` argument).
        let mut arg_types = Vec::new();
        for param in params.iter() {
            arg_types.push(self.analyze_expression(param, parent_function, symbol_table, diagnostics)?.get_type());
        }

        let store_sig = if self.function_table.is_overloaded(&mangled_name) {
            let mut selection_args = Vec::with_capacity(arg_types.len() + 1);
            selection_args.push(struct_name.clone());
            selection_args.extend(arg_types.iter().cloned());
            match self.select_function_overload(&mangled_name, &selection_args) {
                Ok(sig) => sig,
                Err(message) => {
                    diagnostics.report_error(message, Some(method.position.clone()));
                    return Ok(Type::Void);
                }
            }
        } else {
            match self.function_table.get_function(&mangled_name) {
                Ok(s) => s.clone(),
                Err(_) => {
                    diagnostics.report_error(
                        format!("Type '{}' has no method '{}'", struct_name, method.text),
                        Some(method.position.clone()),
                    );
                    return Ok(Type::Void);
                }
            }
        };

        let mut expected_params = store_sig.parameters.clone();
        
        // Remove 'this' from the expected params check since we supply it implicitly
        if !expected_params.is_empty() {
            expected_params.remove(0);
        }

        if expected_params.len() != arg_types.len() {
            diagnostics.report_error(format!("function {} expects {} parameters, got {}", mangled_name, expected_params.len(), arg_types.len()), Some(method.position.clone()));
            return Ok(store_sig.return_type.unwrap_or(Type::Void));
        }

        for (i, given_type) in arg_types.iter().enumerate() {
            let expected_type_str = &expected_params[i];

            if expected_type_str == "object" {
                continue;
            }

            if expected_type_str == "int" && given_type == "float" || expected_type_str == "float" && given_type == "int" || expected_type_str == "double" && given_type == "int" || expected_type_str == "int" && given_type == "double" || expected_type_str == "float" && given_type == "double" || expected_type_str == "double" && given_type == "float" {
                continue;
            }

            if given_type != expected_type_str {
                diagnostics.report_error(format!("function {} expects parameter {} to be {}, got {}", mangled_name, i + 1, expected_type_str, given_type), Some(method.position.clone()));
            }
        }

        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }

    /// `Math.sin/cos/abs/sqrt(x)`: each takes one numeric argument and yields a `float`.
    pub(super) fn analyze_math_call(&mut self, method: &SyntaxToken, params: &Vec<ExpressionNode<'a>>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        if !matches!(method.text.as_str(), "sin" | "cos" | "abs" | "sqrt") {
            diagnostics.report_error(format!("Unknown math function 'Math.{}'", method.text), Some(method.position.clone()));
            return Ok(Type::Float(synthetic_token(TokenKind::DataTypeToken, "float")));
        }
        if params.len() != 1 {
            diagnostics.report_error(format!("'Math.{}' expects exactly 1 argument, got {}", method.text, params.len()), Some(method.position.clone()));
        }
        for param in params.iter() {
            let pt = self.analyze_expression(param, parent_function, symbol_table, diagnostics)?;
            if !matches!(pt.get_type().as_str(), "int" | "float" | "double") {
                diagnostics.report_error(format!("'Math.{}' expects a numeric argument, got {}", method.text, pt.get_type()), param.position());
            }
        }
        Ok(Type::Float(synthetic_token(TokenKind::DataTypeToken, "float")))
    }

    pub(super) fn analyze_break(&mut self,label:&Option<String>,parent_function:&FunctionNode<'a>,has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        if !has_parent_while {
            diagnostics.report_error(
                                  format!("Break statement is not in a loop in function {}",parent_function.name.text), Some(parent_function.name.position.clone()));
        }
        if let Some(name) = label {
            if !self.loop_labels.contains(name) {
                diagnostics.report_error(
                    format!("Break targets unknown loop label '{}'", name), Some(parent_function.name.position.clone()));
            }
        }
        Ok(())
    }
    pub(super) fn analyze_continue(&mut self,label:&Option<String>,parent_function:&FunctionNode<'a>,has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        if !has_parent_while {
            diagnostics.report_error(
                                  format!("Continue statement is not in a loop in function {}",parent_function.name.text), Some(parent_function.name.position.clone()));
        }
        if let Some(name) = label {
            if !self.loop_labels.contains(name) {
                diagnostics.report_error(
                    format!("Continue targets unknown loop label '{}'", name), Some(parent_function.name.position.clone()));
            }
        }
        Ok(())
    }
    pub(super) fn analyze_foreach(&mut self, element:&SyntaxToken, iterable:&ExpressionNode<'a>,
                       index_name:&str, array_name:&str, body:&[StatementNode<'a>],
                       parent_function:&FunctionNode<'a>, symbol_table:&Rc<RefCell<SymbolTable>>,
                       diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        let iterable_type = self.analyze_expression(iterable, parent_function, symbol_table, diagnostics)?;
        let element_type = match &iterable_type {
            Type::Array(inner) => (**inner).clone(),
            _ => {
                diagnostics.report_error(
                    format!("for-each can only iterate over arrays, got {}", iterable_type.get_type()),
                    iterable.position(),
                );
                Type::Void
            }
        };

        // Register the synthetic loop locals plus the user's element binding in a dedicated scope.
        let foreach_scope = Rc::new(RefCell::new(SymbolTable::new(Some(symbol_table.clone()))));
        (*symbol_table).borrow_mut().add_child(foreach_scope.clone());
        {
            let mut scope = (*foreach_scope).borrow_mut();
            let _ = scope.add_symbol(array_name.to_string(), iterable_type.clone());
            let _ = scope.add_symbol(index_name.to_string(), Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int")));
            if let Err(e) = scope.add_symbol(element.text.clone(), element_type) {
                diagnostics.report_error(e.to_string(), Some(element.position.clone()));
            }
        }
        self.analyze_body(body, parent_function, Some(&foreach_scope), true, diagnostics)?;
        Ok(())
    }
    pub(super) fn analyze_switch(&mut self, subject:&ExpressionNode<'a>,
                      cases:&Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
                      default_body:&Option<&'a [StatementNode<'a>]>,
                      parent_function:&FunctionNode<'a>, symbol_table:&Rc<RefCell<SymbolTable>>,
                      has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        let subject_type = self.analyze_expression(subject, parent_function, symbol_table, diagnostics)?;
        let subject_name = subject_type.get_type();
        let subject_is_enum = self.enum_table.contains_key(&subject_name);
        if !matches!(subject_name.as_str(), "int" | "string" | "bool") && !subject_is_enum {
            diagnostics.report_error(
                format!("switch subject must be int, string, bool, or an enum, got {}", subject_name),
                subject.position(),
            );
        }

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (labels, body) in cases.iter() {
            for label in labels.iter() {
                // Labels must be compile-time constants: a literal, or (for enum switches) an
                // enum member access like `Color.Red`.
                let is_enum_label = matches!(label, ExpressionNode::MemberAccess(_, _));
                if !matches!(label, ExpressionNode::Literal(_)) && !is_enum_label {
                    diagnostics.report_error(
                        "switch case labels must be constant literals or enum members".to_string(),
                        label.position(),
                    );
                }
                let label_type = self.analyze_expression(label, parent_function, symbol_table, diagnostics)?;
                self.compare_data_type(&subject_type, &label_type, &empty_span(), diagnostics)?;

                let key = match label {
                    ExpressionNode::Literal(lit) => match lit {
                        Type::Integer(t) | Type::Float(t) | Type::Double(t)
                        | Type::String(t) | Type::Boolean(t) => Some(t.text.clone()),
                        _ => None,
                    },
                    ExpressionNode::MemberAccess(_, m) => Some(m.text.clone()),
                    _ => None,
                };
                if let Some(k) = key {
                    if !seen.insert(k.clone()) {
                        diagnostics.report_error(
                            format!("duplicate case label '{}' in switch statement", k),
                            label.position(),
                        );
                    }
                }
            }
            self.analyze_body(body, parent_function, Some(symbol_table), has_parent_while, diagnostics)?;
        }

        if let Some(db) = default_body {
            self.analyze_body(db, parent_function, Some(symbol_table), has_parent_while, diagnostics)?;
        }
        Ok(())
    }
    pub(super) fn analyze_while(&mut self,condition:&ExpressionNode<'a>,body:&[StatementNode<'a>],
                     parent_function:&FunctionNode<'a>,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        let cond_type = self.analyze_expression(condition,parent_function,symbol_table, diagnostics)?;
        if cond_type.get_type() != "bool" {
            diagnostics.report_error(format!("while condition must be bool, got {}", cond_type.get_type()), condition.position());
        }
        self.analyze_body(body,parent_function,Some(symbol_table),true, diagnostics)?;
        Ok(())
    }
    pub(super) fn analyze_for(&mut self,init:&Option<&'a StatementNode<'a>>,condition:&Option<ExpressionNode<'a>>,
                   increment:&Option<&'a StatementNode<'a>>,body:&[StatementNode<'a>],
                   parent_function:&FunctionNode<'a>,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        let for_scope = Rc::new(RefCell::new(SymbolTable::new(Some(symbol_table.clone()))));
        (*symbol_table).borrow_mut().add_child(for_scope.clone());

        if let Some(init_stmt) = init {
            self.analyze_statement(init_stmt, parent_function, &for_scope, false, diagnostics)?;
        }
        if let Some(cond_expr) = condition {
            let cond_type = self.analyze_expression(cond_expr, parent_function, &for_scope, diagnostics)?;
            if cond_type.get_type() != "bool" {
                diagnostics.report_error(format!("for condition must be bool, got {}", cond_type.get_type()), cond_expr.position());
            }
        }
        if let Some(inc_stmt) = increment {
            self.analyze_statement(inc_stmt, parent_function, &for_scope, false, diagnostics)?;
        }
        self.analyze_body(body, parent_function, Some(&for_scope), true, diagnostics)?;
        Ok(())
    }
    ///return type is returned currently int and float supported
    /// Reports a clear diagnostic when a reserved word (a builtin name or primitive type name) is
    /// used where a user-chosen identifier is expected (`role` is e.g. "variable"/"function").
    pub(super) fn check_reserved_name(&self, token: &SyntaxToken, role: &str, diagnostics: &mut DiagnosticBag) {
        // bare callable, so it is a legal ordinary identifier.
        const RESERVED_NAMES: &[&str] = &[
            "print", "println", "to_string", "hash_code", "array_new", "Math",
            "int", "float", "double", "string", "bool", "char", "object", "void",
            // C#/.NET-style aliases for the primitives (see `canonical_type_name`).
            "String", "Int32", "Int64", "Single", "Double", "Boolean", "Char", "Object", "Void",
            "true", "false", "null",
        ];
        if RESERVED_NAMES.contains(&token.text.as_str()) {
            diagnostics.report_error(
                format!("'{}' is a reserved word and cannot be used as a {} name", token.text, role),
                Some(token.position.clone()),
            );
        }
    }

    pub(super) fn analyze_declaration(&mut self,left:&SyntaxToken, type_annotation: &Option<Type>, right:&ExpressionNode<'a>, is_const: bool, parent_function:&FunctionNode<'a>,
                           symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        self.check_reserved_name(left, "variable", diagnostics);
        // Empty array literals carry no element type, so the declaration must supply one via an
        // array-typed annotation (e.g. `let xs: int[] = [];`).
        if let ExpressionNode::ArrayLiteral(elements) = right {
            if elements.is_empty() {
                match type_annotation {
                    Some(t) if t.is_array() => {
                        if let Err(e) = (*symbol_table).as_ref().borrow_mut().add_symbol(left.text.clone(), t.clone()) {
                            diagnostics.report_error(e.to_string(), Some(left.position.clone()));
                        }
                        if is_const {
                            (*symbol_table).as_ref().borrow_mut().mark_const(left.text.clone());
                        }
                        return Ok(());
                    }
                    _ => {
                        diagnostics.report_error(
                            "Empty array literal requires an array type annotation, e.g. `let xs: int[] = [];`".to_string(),
                            Some(left.position.clone()),
                        );
                        return Ok(());
                    }
                }
            }
        }
        //return right type
        let right_type=self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;
        
        let var_type = if let Some(t) = type_annotation {
            self.compare_data_type(t, &right_type, &left.position, diagnostics)?;
            t.clone()
        } else {
            right_type.clone()
        };
        
        if let Err(e) = (*symbol_table).as_ref().borrow_mut().add_symbol(left.text.clone(), var_type) {
            diagnostics.report_error(e.to_string(), Some(left.position.clone()));
        }
        if is_const {
            (*symbol_table).as_ref().borrow_mut().mark_const(left.text.clone());
        }
        Ok(())
    }
    pub(super) fn analyze_assignment(&mut self,left:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                          symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        if (*symbol_table).as_ref().borrow().is_const(&left.text) {
            diagnostics.report_error(
                format!("Cannot assign to '{}' because it is a const binding", left.text),
                Some(left.position.clone()),
            );
        }
        let r=self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;
        let l = match (*symbol_table).as_ref().borrow().get_symbol(left) {
            Ok(sym) => sym,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(left.position.clone()));
                return Ok(());
            }
        };
        self.compare_data_type(&l,&r,&left.position, diagnostics)?;
        Ok(())
    }
    
    pub(super) fn analyze_index_assignment(&mut self, arr: &ExpressionNode<'a>, index: &ExpressionNode<'a>, right: &ExpressionNode<'a>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let array_type = self.analyze_expression(arr, parent_function, symbol_table, diagnostics)?;

        let inner_type = match array_type {
            Type::Array(inner) => *inner,
            _ => {
                diagnostics.report_error(format!("Cannot index into non-array type {}", array_type.get_type()), arr.position());
                return Ok(());
            }
        };

        let index_type = self.analyze_expression(index, parent_function, symbol_table, diagnostics)?;
        if index_type.get_type() != "int" {
            diagnostics.report_error(format!("Array index must be of type int, got {}", index_type.get_type()), index.position());
        }

        let right_type = self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&inner_type, &right_type, &empty_span(), diagnostics)?;
        
        Ok(())
    }

    pub(super) fn analyze_member_assignment(&mut self, obj: &ExpressionNode<'a>, member: &SyntaxToken, right: &ExpressionNode<'a>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

        let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
            Some(parts) => parts,
            None => {
                diagnostics.report_error(format!("Cannot access member of non-class type {}", obj_type.get_type()), Some(member.position.clone()));
                return Ok(());
            }
        };

        self.ensure_struct_instantiated(&base_name, &generic_args, &member.position, diagnostics);
        let struct_name = mangle_generic(&base_name, &generic_args);

        let field_type = {
            let struct_info = match self.struct_table.get_struct(&struct_name) {
                Some(info) => info,
                None => {
                    diagnostics.report_error(format!("Struct '{}' not found", struct_name), Some(member.position.clone()));
                    return Ok(());
                }
            };

            match struct_info.fields.get(&member.text) {
                Some(info) => info.type_.clone(),
                None => {
                    diagnostics.report_error(format!("Field '{}' not found in class '{}'", member.text, struct_name), Some(member.position.clone()));
                    return Ok(());
                }
            }
        };

        // Private fields (`_name`) may only be written from within the declaring type's methods.
        if member.text.starts_with('_') && !self.in_methods_of(parent_function, &base_name) {
            diagnostics.report_error(
                format!("'{}' is private to '{}'", member.text, base_name),
                Some(member.position.clone()),
            );
        }

        let right_type = self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&field_type, &right_type, &member.position, diagnostics)?;
        
        Ok(())
    }
    pub(super) fn analyze_expression(&mut self,expression:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                          symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        match expression
        {
            ExpressionNode::Literal(number) =>
                Ok(number.clone()),
            ExpressionNode::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    diagnostics.report_error("Empty array literals are not supported yet".to_string(), None);
                    return Ok(Type::Array(Box::new(Type::Void)));
                }
                
                let first_type = self.analyze_expression(&elements[0], parent_function, symbol_table, diagnostics)?;
                
                for i in 1..elements.len() {
                    let element_type = self.analyze_expression(&elements[i], parent_function, symbol_table, diagnostics)?;
                    self.compare_data_type(&first_type, &element_type, &empty_span(), diagnostics)?;
                }
                
                Ok(Type::Array(Box::new(first_type)))
            },
            ExpressionNode::IndexAccess(array_expr, index_expr) => {
                let array_type = self.analyze_expression(array_expr, parent_function, symbol_table, diagnostics)?;
                let inner_type = match array_type {
                    Type::Array(inner) => *inner,
                    _ => {
                        diagnostics.report_error(format!("Cannot index into non-array type {}", array_type.get_type()), array_expr.position());
                        Type::Void
                    }
                };
                
                let index_type = self.analyze_expression(index_expr, parent_function, symbol_table, diagnostics)?;
                if index_type.get_type() != "int" {
                    diagnostics.report_error(format!("Array index must be of type int, got {}", index_type.get_type()), index_expr.position());
                }
                
                Ok(inner_type)
            },
            ExpressionNode::Unary(opr,right)=> {
                let right_type = self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;
                match opr.kind {
                    TokenKind::BangToken => {
                        if right_type.get_type() != "bool" {
                            diagnostics.report_error(format!("! operator requires bool, got {}", right_type.get_type()), Some(opr.position.clone()));
                        }
                        Ok(Type::Boolean(opr.clone()))
                    },
                    TokenKind::PlusToken | TokenKind::MinusToken => {
                        if right_type.get_type() != "int" && right_type.get_type() != "float" {
                            diagnostics.report_error(format!("unary +/- requires int or float, got {}", right_type.get_type()), Some(opr.position.clone()));
                        }
                        Ok(right_type)
                    },
                    _ => {
                        diagnostics.report_error(format!("unknown unary operator {}", opr.text), Some(opr.position.clone()));
                        Ok(right_type)
                    }
                }
            },
            ExpressionNode::Binary(left,opr,right)=>
                Ok(self.analyze_binary_expression(left,opr,right,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table, diagnostics)?),
            ExpressionNode::FunctionCall(name,generic_args,params)=>
                Ok(self.analyze_function_call(name,generic_args,params,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::IsExpression(left, _right_type) => {
                // `is` always evaluates to a bool; the actual comparison is resolved at compile time.
                self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                Ok(Type::Boolean(synthetic_token(TokenKind::BooleanToken, "true")))
            },
            ExpressionNode::Parenthesized(expr)=>
                Ok(self.analyze_expression(expr,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::Ternary(condition, then_expr, else_expr) => {
                let cond_type = self.analyze_expression(condition, parent_function, symbol_table, diagnostics)?;
                if cond_type.get_type() != "bool" {
                    diagnostics.report_error(
                        format!("Ternary condition must be of type bool, got {}", cond_type.get_type()),
                        condition.position(),
                    );
                }
                let then_type = self.analyze_expression(then_expr, parent_function, symbol_table, diagnostics)?;
                let else_type = self.analyze_expression(else_expr, parent_function, symbol_table, diagnostics)?;
                // Both branches must agree; reuse the standard compatibility check.
                self.compare_data_type(&then_type, &else_type, &empty_span(), diagnostics)?;
                Ok(then_type)
            },
            ExpressionNode::StructInstantiation(name, generic_args, fields) => {
                // Resolve generic type arguments through the active monomorphization bindings so a
                // `List<T>{...}` written inside a generic function/method body instantiates the
                // concrete `List<int>` rather than a stray `List<T>`.
                let resolved_args: Vec<Type> = generic_args.as_deref().unwrap_or(&[]).iter()
                    .map(|a| Self::monomorphize_type(a, &self.current_generic_bindings))
                    .collect();
                let generic_args_slice = resolved_args.as_slice();
                let struct_name = mangle_generic(&name.text, generic_args_slice);

                // Monomorphize generic struct if needed
                self.ensure_struct_instantiated(&name.text, generic_args_slice, &name.position, diagnostics);
                
                let struct_info = match self.struct_table.get_struct(&struct_name) {
                    Some(info) => info.clone(),
                    None => {
                        diagnostics.report_error(format!("Struct '{}' not found", struct_name), Some(name.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                // Check that all fields are provided and types match
                let mut provided_fields = std::collections::HashSet::new();
                for (field_name, field_expr) in fields {
                    provided_fields.insert(field_name.text.clone());
                    
                    let field_info = match struct_info.fields.get(&field_name.text) {
                        Some(info) => info,
                        None => {
                            diagnostics.report_error(format!("Field '{}' not found in class '{}'", field_name.text, struct_name), Some(field_name.position.clone()));
                            continue;
                        }
                    };

                    let expr_type = self.analyze_expression(field_expr, parent_function, symbol_table, diagnostics)?;
                    self.compare_data_type(&field_info.type_, &expr_type, &field_name.position, diagnostics)?;
                }

                // Check for missing fields
                for expected_field in struct_info.fields.keys() {
                    if !provided_fields.contains(expected_field) {
                        diagnostics.report_error(format!("Missing field '{}' in class instantiation of '{}'", expected_field, struct_name), Some(name.position.clone()));
                    }
                }

                let mut dummy_token = name.clone();
                dummy_token.text = struct_name.clone();
                Ok(Type::Struct(dummy_token, None))
            },
            ExpressionNode::MemberAccess(obj, member) => {
                // Enum member access `EnumName.Member` resolves to the enum type (an i32 at runtime).
                if let ExpressionNode::Identifier(id) = obj {
                    if self.enum_table.contains_key(&id.text) {
                        if self.enum_member_value(&id.text, &member.text).is_none() {
                            diagnostics.report_error(
                                format!("Enum '{}' has no member '{}'", id.text, member.text),
                                Some(member.position.clone()),
                            );
                        }
                        return Ok(Type::Struct(id.clone(), None));
                    }
                }
                let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

                let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
                    Some(parts) => parts,
                    None => {
                        diagnostics.report_error(format!("Cannot access member of non-class type {}", obj_type.get_type()), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                self.ensure_struct_instantiated(&base_name, &generic_args, &member.position, diagnostics);
                let struct_name = mangle_generic(&base_name, &generic_args);

                let struct_info = match self.struct_table.get_struct(&struct_name) {
                    Some(info) => info,
                    None => {
                        diagnostics.report_error(format!("Struct '{}' not found", struct_name), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                let field_info = match struct_info.fields.get(&member.text) {
                    Some(info) => info,
                    None => {
                        diagnostics.report_error(format!("Field '{}' not found in class '{}'", member.text, struct_name), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                let field_type = field_info.type_.clone();

                // Private fields (`_name`) may only be read from within the declaring type's methods.
                if member.text.starts_with('_') && !self.in_methods_of(parent_function, &base_name) {
                    diagnostics.report_error(
                        format!("'{}' is private to '{}'", member.text, base_name),
                        Some(member.position.clone()),
                    );
                }

                Ok(field_type)
            },
            ExpressionNode::Cast(target_type, expr) => {
                let expr_type = self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?;
                
                let target_type_str = target_type.get_type();
                let expr_type_str = expr_type.get_type();

                // If the target (after peeling array wrappers) is a generic struct, instantiate it.
                let mut core_target = target_type;
                while let Type::Array(inner) = core_target {
                    core_target = inner;
                }
                if let Some((base_name, generic_args)) = Self::resolve_struct_parts(core_target) {
                    self.ensure_struct_instantiated(&base_name, &generic_args, &empty_span(), diagnostics);
                }

                // Allow int <-> float casts
                if (target_type_str == "int" && expr_type_str == "float") ||
                   (target_type_str == "float" && expr_type_str == "int") ||
                   (target_type_str == "double" && expr_type_str == "int") ||
                   (target_type_str == "int" && expr_type_str == "double") ||
                   (target_type_str == "float" && expr_type_str == "double") ||
                   (target_type_str == "double" && expr_type_str == "float") ||
                   // `char` is a code point: allow lossless conversion to/from `int`.
                   (target_type_str == "char" && expr_type_str == "int") ||
                   (target_type_str == "int" && expr_type_str == "char") {
                    Ok(target_type.clone())
                } else if target_type_str == expr_type_str {
                    Ok(target_type.clone())
                } else if target_type_str == "object" || expr_type_str == "object" {
                    // Boxing (`T as object`) and unboxing (`object as T`) are always permitted;
                    // an unbox to the wrong primitive traps at runtime.
                    Ok(target_type.clone())
                } else if expr_type_str == "int" && (self.struct_table.get_struct(&target_type_str).is_some() || target_type_str.ends_with("[]") || target_type_str.ends_with("?")) {
                    // Allow casting int to pointer types (for null pointers)
                    Ok(target_type.clone())
                } else {
                    diagnostics.report_error(format!("Cannot cast from {} to {}", expr_type_str, target_type_str), target_type.get_span().or_else(|| expr.position()));
                    Ok(target_type.clone())
                }
            },
            ExpressionNode::MethodCall(obj, method, generic_args, params) => self.analyze_method_call(obj, method, generic_args, params, parent_function, symbol_table, diagnostics),
            ExpressionNode::Await(inner) => {
                let fut = self.analyze_expression(inner, parent_function, symbol_table, diagnostics)?;
                match Self::future_inner_type(&fut) {
                    Some(t) => Ok(t),
                    None => {
                        diagnostics.report_error(
                            format!("'await' expects a Future value, got {}", fut.get_type()),
                            inner.position(),
                        );
                        Ok(Type::Void)
                    }
                }
            },
        }
    }
    pub(super) fn analyze_binary_expression(&mut self,left:&ExpressionNode<'a>,opr:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                                 symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        let left_value = self.analyze_expression(left,parent_function,symbol_table, diagnostics)?;
        let right_value = self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;

        // Null-coalescing `a ?? b`: `a` should be nullable; the result is the unwrapped element
        // type, and `b` must be assignable to it (or itself nullable of the same element type).
        if opr.kind == TokenKind::QuestionQuestionToken {
            let result_type = match &left_value {
                Type::Nullable(inner) => (**inner).clone(),
                other => other.clone(),
            };
            let right_unwrapped = match &right_value {
                Type::Nullable(inner) => (**inner).clone(),
                other => other.clone(),
            };
            self.compare_data_type(&result_type,&right_unwrapped,&opr.position, diagnostics)?;
            return Ok(result_type);
        }

        self.compare_data_type(&left_value,&right_value,&opr.position, diagnostics)?;
        match (&left_value,&opr.kind) {
          (Type::String(_),TokenKind::PlusToken)=> {}
          // Reference (identity) equality is allowed on strings and objects.
          (Type::String(_),TokenKind::EqualEqualToken)|(Type::String(_),TokenKind::NotEqualToken)=> {}
          (Type::String(_),_)=> {
              diagnostics.report_error(format!("Cannot perform operation {} on string",opr.text), Some(opr.position.clone()));
          }
            (_,_)=>{}
        };
        
        match opr.kind {
            TokenKind::EqualEqualToken | TokenKind::NotEqualToken |
            TokenKind::GreaterThanToken | TokenKind::GreaterThanEqualToken |
            TokenKind::SmallerThanToken | TokenKind::SmallerThanEqualToken |
            TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken => {
                return Ok(Type::Boolean(opr.clone()));
            },
            _ => return Ok(left_value)
        }
    }
    pub(super) fn compare_data_type(&mut self, left:&Type, right:&Type, position:&TextSpan, diagnostics: &mut DiagnosticBag) ->Result<(),()> {
        if left.get_type() == right.get_type() {
            return Ok(())
        }
        if self.enum_int_compatible(&left.get_type(), &right.get_type()) {
            return Ok(())
        }

        // Any value may be assigned (boxed) into an `object` target; the reverse requires a
        // cast and is rejected here.
        if left.get_type() == "object" {
            return Ok(());
        }
        
        // A nullable `T?` accepts another `T?`, a plain `T`, or the `null` literal (`void?`).
        if let Type::Nullable(inner) = left {
            if let Type::Nullable(inner_right) = right {
                if inner.get_type() == inner_right.get_type() {
                    return Ok(());
                }
            } else if inner.get_type() == right.get_type() {
                return Ok(());
            }
            if right.get_type() == "void?" {
                return Ok(());
            }
        }

        // Any reference type (or nullable) can be compared against the `null` literal.
        if (left.get_type().ends_with("?") || self.is_reference_type(&left.get_type())) && right.get_type() == "void?" {
            return Ok(());
        }
        if (right.get_type().ends_with("?") || self.is_reference_type(&right.get_type())) && left.get_type() == "void?" {
            return Ok(());
        }
        
        diagnostics.report_error(format!("cannot convert from {} to {} at {}",
                       left.get_type(),right.get_type(),position.get_point_str()), Some(position.clone()));
        Ok(())
    }

    pub fn is_reference_type(&self, type_name: &str) -> bool {
        if self.struct_table.is_reference_type(type_name) {
            return true;
        }
        // A not-yet-instantiated generic struct instance (e.g. `Box_int`) is also a reference type.
        let base_name = strip_nullable(type_name);
        self.demangle_generic_struct(base_name).is_some()
    }
    pub(super) fn analyze_identifier(&mut self,id:&SyntaxToken,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        let r= match (*symbol_table).as_ref().borrow().get_symbol(id) {
            Ok(t) => t,
            Err(e) => {
                // A bare identifier that names a top-level function is a first-class function value.
                if let Ok(sig) = self.function_table.get_function(&id.text) {
                    let params = sig.parameters.iter().map(|p| Self::type_from_name(p)).collect();
                    let ret = sig.return_type.clone().unwrap_or(Type::Void);
                    return Ok(Type::Function(params, Box::new(ret)));
                }
                diagnostics.report_error(e.to_string(), Some(id.position.clone()));
                Type::Void
            }
        };
        Ok(r)
    }

    /// Reconstructs a `Type` from its canonical type-name string (as stored in function-table
    /// signatures), e.g. "int", "string", "Node", "int[]". Falls back to `void` if unparseable.
    pub(super) fn type_from_name(name: &str) -> Type {
        let token = synthetic_token(TokenKind::IdentifierToken, name);
        Type::from_token(token).unwrap_or(Type::Void)
    }

    pub(super) fn analyze_if_else(&mut self, condition:&ExpressionNode<'a>, if_body:&[StatementNode<'a>],
                       else_if:&Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>,
                       else_body: &Option<&'a [StatementNode<'a>]>,
                       parent_function:&FunctionNode<'a>, symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool, diagnostics: &mut DiagnosticBag) ->
    Result<(),()>
    {
        // Check for constant expression from `is`
        let mut is_constant_true = false;
        let mut is_constant_false = false;
        
        if let ExpressionNode::IsExpression(left, right_type) = condition {
            let left_t = self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
            // `is` on an `object` is a runtime check; only non-object operands fold to a constant.
            if left_t.get_type() != "object" {
                if left_t.get_type() == right_type.get_type() {
                    is_constant_true = true;
                } else {
                    is_constant_false = true;
                }
            }
        }
        
        if !is_constant_false {
            //if condition
            let cond_type = self.analyze_expression(condition,parent_function,symbol_table, diagnostics)?;
            if cond_type.get_type() != "bool" {
                diagnostics.report_error(format!("if condition must be bool, got {}", cond_type.get_type()), condition.position());
            }
            //if body
            self.analyze_body(if_body,parent_function,Some(symbol_table),has_parent_while, diagnostics)?;
        }
        
        if is_constant_true {
            return Ok(());
        }

        //else if block
        for i in else_if.iter()
        {
            let mut elif_constant_true = false;
            let mut elif_constant_false = false;
            if let ExpressionNode::IsExpression(left, right_type) = &i.0 {
                let left_t = self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                if left_t.get_type() != "object" {
                    if left_t.get_type() == right_type.get_type() {
                        elif_constant_true = true;
                    } else {
                        elif_constant_false = true;
                    }
                }
            }

            if !elif_constant_false {
                let elif_cond_type = self.analyze_expression(&i.0,parent_function,symbol_table, diagnostics)?;
                if elif_cond_type.get_type() != "bool" {
                    diagnostics.report_error(format!("else if condition must be bool, got {}", elif_cond_type.get_type()), i.0.position());
                }
                self.analyze_body(&i.1,parent_function,Some(symbol_table),has_parent_while, diagnostics)?;
            }
            
            if elif_constant_true {
                return Ok(());
            }
        }
        
        match else_body
        {
            Some(body)=>self.analyze_body(body,parent_function,Some(symbol_table),has_parent_while, diagnostics)?,
            None=>()
        }
        Ok(())
    }
    pub(super) fn analyze_return(&mut self,expression:&Option<ExpressionNode<'a>>,parent_function:&FunctionNode<'a>,
                      symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        match (expression,&parent_function.return_type)
        {
            (Some(expression),&Some(ref return_type))=>
            {
                let r=self.analyze_expression(expression,parent_function,symbol_table, diagnostics)?;
                self.compare_data_type(return_type, &r, &parent_function.name.position, diagnostics)?;
            },
            (None,&Some(_))=> {
                diagnostics.report_error(format!("return type mismatch at  {}",parent_function.name.position.get_point_str()), Some(parent_function.name.position.clone()));
            },
            (Some(_),&None)=> {
                diagnostics.report_error(format!("return type mismatch at {}",parent_function.name.position.get_point_str()), Some(parent_function.name.position.clone()));
            },
            (None,&None)=>()
        };
        Ok(())
    }

}
