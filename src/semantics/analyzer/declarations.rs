use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::semantics::function_table::FunctionTableInfo;
use crate::semantics::symbol_table::SymbolTable;
use crate::syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use crate::syntax::nodes::types::{mangle_generic, method_fn, strip_array, strip_nullable};
use crate::syntax::nodes::{FunctionNode, ProgramNode, Type};
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// Pass: register every enum and its members (member -> integer value), reporting duplicate
    /// enum names and duplicate member names.
    pub(super) fn register_enums(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for enum_decl in node.enums.iter() {
            if self.enum_table.contains_key(&enum_decl.name.text) {
                diagnostics.report_error(
                    format!("Enum '{}' is already defined", enum_decl.name.text),
                    Some(enum_decl.name.position),
                );
                continue;
            }
            let mut members = HashMap::new();
            for (member, value) in enum_decl.members.iter() {
                if members.contains_key(&member.text) {
                    diagnostics.report_error(
                        format!(
                            "Duplicate member '{}' in enum '{}'",
                            member.text, enum_decl.name.text
                        ),
                        Some(member.position),
                    );
                    continue;
                }
                members.insert(member.text.clone(), *value);
            }
            self.enum_table.insert(enum_decl.name.text.clone(), members);
        }
    }

    /// Returns the integer value of an enum member, if `enum_name.member` names a known enum member.
    pub(super) fn enum_member_value(&self, enum_name: &str, member: &str) -> Option<i32> {
        self.enum_table
            .get(enum_name)
            .and_then(|m| m.get(member))
            .copied()
    }

    /// Enum-typed values are integers at runtime, so an enum type and `int` are mutually
    /// assignable/comparable (C-style). Used to relax type checks involving enums.
    pub(super) fn enum_int_compatible(&self, a: &str, b: &str) -> bool {
        (self.enum_table.contains_key(a) && b == "int")
            || (self.enum_table.contains_key(b) && a == "int")
    }

    /// Pass 0: register every (non-generic) struct and its methods; stash generic templates.
    pub(super) fn register_structs(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for struct_decl in node.structs.iter() {
            diagnostics.file_path = file_path_string(&struct_decl.file_path);
            if struct_decl.generic_parameters.is_some() {
                // v1 restriction: async methods on generic classes are not supported (the async
                // state machine would have to be re-generated per monomorphization).
                for method in struct_decl.methods.iter() {
                    if method.is_async {
                        diagnostics.report_error(
                            format!("Async methods are not supported on generic class '{}' (method '{}')", struct_decl.name.text, method.name.text),
                            Some(method.name.position),
                        );
                    }
                }
                self.generic_structs
                    .insert(struct_decl.name.text.clone(), struct_decl);
                continue;
            }
            if let Err(e) = self.struct_table.add_struct(struct_decl) {
                diagnostics.report_error(e, Some(struct_decl.name.position));
            }
            self.register_struct_methods(struct_decl, &struct_decl.name.text, &[], diagnostics);
        }
    }

    /// Pass: analyze and register every top-level variable. Each initializer is type-checked in
    /// declaration order against the globals declared so far (forward references to later globals
    /// are not allowed) plus all already-registered functions/types. The resolved type is recorded
    /// in the module-global symbol scope so function bodies can resolve the variable, and surfaced
    /// to codegen via [`super::GlobalSymbol`].
    pub(super) fn register_globals(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        // A synthetic, parameterless, non-async "module init" supplies the parent-function context
        // that expression analysis requires; with no `this` parameter it is treated as outside any
        // type, so initializers cannot reach private members.
        let empty_body: &'a [crate::syntax::nodes::StatementNode<'a>] = &[];
        let init_fn = FunctionNode::new(
            Vec::new(),
            synthetic_token(TokenKind::IdentifierToken, "__module_init"),
            None,
            None,
            Vec::new(),
            empty_body,
            false,
        );

        for global in node.globals.iter() {
            diagnostics.file_path = file_path_string(&global.file_path);
            self.check_reserved_name(&global.name, "variable", diagnostics);

            if global.is_public && global.is_static {
                diagnostics.report_error(
                    format!(
                        "Top-level variable '{}' cannot be both 'public' and 'static'",
                        global.name.text
                    ),
                    Some(global.name.position),
                );
            }

            if self.globals.iter().any(|g| g.name == global.name.text) {
                diagnostics.report_error(
                    format!("Top-level variable '{}' is already defined", global.name.text),
                    Some(global.name.position),
                );
                continue;
            }

            let gtable = self.global_symbol_table.clone();
            let init_type = self
                .analyze_expression(&global.initializer, &init_fn, &gtable, diagnostics)
                .unwrap_or(Type::Void);

            let resolved = match &global.declared_type {
                Some(declared) => {
                    let dt = declared.get_type();
                    let it = init_type.get_type();
                    let numeric = crate::syntax::nodes::types::is_numeric_primitive(&dt)
                        && crate::syntax::nodes::types::is_numeric_primitive(&it);
                    if !numeric && it != "void" && !self.type_str_assignable(&dt, &it) {
                        diagnostics.report_error(
                            format!(
                                "Top-level variable '{}' is declared '{}' but initialized with '{}'",
                                global.name.text, dt, it
                            ),
                            Some(global.name.position),
                        );
                    }
                    declared.clone()
                }
                None => init_type,
            };

            {
                let mut table = self.global_symbol_table.borrow_mut();
                let _ = table.add_symbol(global.name.text.clone(), resolved.clone());
                if global.is_const {
                    table.mark_const(global.name.text.clone());
                }
            }

            self.globals.push(super::GlobalSymbol {
                name: global.name.text.clone(),
                type_str: resolved.get_type(),
                is_const: global.is_const,
                is_public: global.is_public,
                is_static: global.is_static,
            });
        }
    }

    /// Pass 1: register every (non-generic) function signature; stash generic templates.
    pub(super) fn register_functions(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for function in node.functions.iter() {
            diagnostics.file_path = file_path_string(&function.file_path);
            self.check_reserved_name(&function.name, "function", diagnostics);
            if function.generic_parameters.is_some() {
                self.generic_functions
                    .insert(function.name.text.clone(), function);
                continue;
            }
            if function.is_public {
                self.check_public_visibility(function, diagnostics);
            }
            if let Err(e) = self
                .function_table
                .add_overload(&function.name.text, FunctionTableInfo::from(function))
            {
                diagnostics.report_error(e.to_string(), Some(function.name.position));
            }
        }
        // The entry point is exported under the fixed name `main`. It may be declared as `main()`
        // or `main(args: string[])`, but not overloaded or given any other signature.
        if self.function_table.is_overloaded("main") {
            diagnostics.report_error("'main' cannot be overloaded".to_string(), None);
        } else if let Ok(info) = self.function_table.get_function(&"main".to_string()) {
            let ok = info.parameters.is_empty()
                || (info.parameters.len() == 1 && info.parameters[0] == "string[]");
            if !ok {
                diagnostics.report_error(
                    "'main' must be declared as 'main()' or 'main(args: string[])'".to_string(),
                    None,
                );
            }
        }
    }

    /// Ensures a `public` function does not leak a private (non-`public`) class through its
    /// signature, which would make the class unusable by the callers the function is exposed to.
    pub(super) fn check_public_visibility(
        &self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let signature_types = function
            .return_type
            .iter()
            .chain(function.parameters.iter().map(|p| &p.type_));
        for type_to_check in signature_types {
            let base_type_str = strip_nullable(strip_array(&type_to_check.get_type())).to_string();
            if let Some(struct_info) = self.struct_table.get_struct(&base_type_str) {
                if !struct_info.is_public {
                    diagnostics.report_error(
                        format!(
                            "Public function '{}' exposes private class '{}'",
                            function.name.text, base_type_str
                        ),
                        Some(function.name.position),
                    );
                }
            }
        }
    }

    /// Pass 2: analyze the body of every concrete function.
    pub(super) fn analyze_function_bodies(
        &mut self,
        node: &'a ProgramNode<'a>,
        symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                continue;
            }
            // Extern functions have no body; their signature is enough for call-site checks.
            if function.is_extern {
                continue;
            }
            diagnostics.file_path = file_path_string(&function.file_path);
            let table = self.analyze_function(function, diagnostics)?;
            // Key the symbol table by the emitted name so overloaded functions (which share a
            // base name but emit distinct mangled names) each get their own entry, matching the
            // name codegen uses.
            let param_types: Vec<String> = function
                .parameters
                .iter()
                .map(|p| p.type_.get_type())
                .collect();
            let key = self
                .function_table
                .resolve_emitted_name(&function.name.text, &param_types);
            symbol_table_map.insert(key, table);
        }
        Ok(())
    }

    /// Passes 3 & 4 (combined fixpoint): analyze the bodies of every monomorphized generic
    /// function instance and every (de-sugared) struct method.
    ///
    /// Analyzing one body can lazily instantiate *more* generics — a struct method that uses
    /// `List<JsonValue>` queues new struct methods, and a builder that calls `List<JsonValue>()`
    /// queues a new generic function instance. The two feed each other, so we loop until neither
    /// the generic-function set nor the struct-method list grows. Both instantiation paths are
    /// idempotent (guarded by the struct/function tables), so this terminates.
    pub(super) fn analyze_pending_instantiations(
        &mut self,
        symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), ()> {
        let mut processed_generics: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut method_index = 0;
        loop {
            let mut progressed = false;

            // Monomorphized generic function instances (e.g. `List<JsonValue>`, `swap_int_string`).
            let pending: Vec<String> = self
                .instantiated_generics
                .keys()
                .filter(|k| !processed_generics.contains(*k))
                .cloned()
                .collect();
            for mangled_name in pending {
                processed_generics.insert(mangled_name.clone());
                let (bindings, template) = match self.instantiated_generics.get(&mangled_name) {
                    Some((b, t)) => (b.clone(), *t),
                    None => continue,
                };
                diagnostics.file_path = file_path_string(&template.file_path);
                self.current_generic_bindings = bindings;
                let table = self.analyze_function(template, diagnostics)?;
                self.current_generic_bindings = Vec::new();
                symbol_table_map.insert(mangled_name, table);
                progressed = true;
            }

            // De-sugared struct methods, including those for newly instantiated generic structs.
            while method_index < self.struct_methods.len() {
                let (method, bindings) = self.struct_methods[method_index].clone();
                method_index += 1;
                diagnostics.file_path = file_path_string(&method.file_path);
                self.current_generic_bindings = bindings;
                let table = self.analyze_function(method, diagnostics)?;
                self.current_generic_bindings = Vec::new();
                // Key by the emitted name so overloaded methods each get a distinct entry (the
                // parameter list includes the implicit `this`).
                let param_types: Vec<String> = method
                    .parameters
                    .iter()
                    .map(|p| p.type_.get_type())
                    .collect();
                let key = self
                    .function_table
                    .resolve_emitted_name(&method.name.text, &param_types);
                symbol_table_map.insert(key, table);
                progressed = true;
            }

            if !progressed {
                break;
            }
        }
        Ok(())
    }
    pub(super) fn ensure_struct_instantiated(
        &mut self,
        base_name: &str,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let mangled_name = mangle_generic(base_name, args);
        if self.struct_table.get_struct(&mangled_name).is_some() {
            return;
        }

        let template = match self.generic_structs.get(base_name) {
            Some(template) => *template,
            None => return,
        };

        let params = template.generic_parameters.as_deref().unwrap_or(&[]);
        if args.len() != params.len() {
            diagnostics.report_error(
                format!(
                    "Generic class '{}' expects {} type argument(s), but {} were provided",
                    base_name,
                    params.len(),
                    args.len()
                ),
                Some(*position),
            );
        }
        let bindings = generic_bindings(params, args);

        let new_fields: Vec<StructFieldNode> = template
            .fields
            .iter()
            .map(|field| StructFieldNode {
                attributes: field.attributes.clone(),
                name: field.name.clone(),
                is_public: field.is_public,
                type_token: substitute_generic_token(&field.type_token, &bindings),
                field_type: substitute_generic_type(&field.field_type, &bindings),
            })
            .collect();

        let mut new_name_token = template.name.clone();
        new_name_token.text = mangled_name.clone();
        let new_decl = StructDeclarationNode::new(
            template.attributes.clone(),
            new_name_token,
            None,
            new_fields,
            template.methods.clone(),
            template.is_public,
        );

        let new_decl_ref: &'a StructDeclarationNode<'a> = self.arena.alloc(new_decl);

        if let Err(e) = self.struct_table.add_struct(new_decl_ref) {
            diagnostics.report_error(e, Some(*position));
        }

        self.register_struct_methods(new_decl_ref, &mangled_name, &bindings, diagnostics);
    }

    pub(super) fn register_struct_methods(
        &mut self,
        struct_decl: &'a StructDeclarationNode<'a>,
        struct_type_str: &str,
        bindings: &[(String, String)],
        diagnostics: &mut DiagnosticBag,
    ) {
        self.register_methods_for(struct_type_str, &struct_decl.methods, bindings, diagnostics);
    }

    /// Registers a list of methods against `target_type_str` (a struct, a monomorphized generic
    /// struct, or a primitive/`object` extended via an `extend` block). Each method is renamed to
    /// `{target}_{method}`, given an implicit `this` parameter of the target type, queued for
    /// codegen, and recorded in the function table. Shared by struct declarations and `extend`
    /// blocks so they lower identically.
    pub(super) fn register_methods_for(
        &mut self,
        target_type_str: &str,
        methods: &'a [FunctionNode<'a>],
        bindings: &[(String, String)],
        diagnostics: &mut DiagnosticBag,
    ) {
        for method in methods {
            // Validate object-protocol overrides once (on the non-monomorphized declaration).
            if bindings.is_empty() {
                self.validate_protocol_override(method, diagnostics);
            }
            let mangled_name = method_fn(target_type_str, &method.name.text);

            if method.generic_parameters.is_some() {
                self.generic_functions.insert(mangled_name.clone(), method);
            }

            let mut new_method = method.clone();
            new_method.name = synthetic_token(TokenKind::IdentifierToken, &mangled_name);

            if !bindings.is_empty() {
                Self::substitute_generic_signature(&mut new_method, bindings);
            }

            // Static methods have no implicit receiver; instance methods get `this` at index 0.
            if !new_method.is_static {
                new_method
                    .parameters
                    .insert(0, Self::make_this_param(target_type_str));
            }

            let method_ref = self.arena.alloc(new_method);
            self.struct_methods.push((method_ref, bindings.to_vec()));

            if let Err(e) = self
                .function_table
                .add_overload(&mangled_name, FunctionTableInfo::from(method_ref))
            {
                diagnostics.report_error(e.to_string(), Some(method.name.position));
            }
        }
    }

    /// Returns true if `name` is a type that an `extend` block may attach methods to: a
    /// primitive, `object`, a registered struct, a generic struct template, or an enum.
    pub(super) fn is_extendable_target(&self, name: &str) -> bool {
        matches!(
            name,
            "int" | "float" | "double" | "string" | "bool" | "char" | "object" | "JsRef"
        ) || self.struct_table.get_struct(name).is_some()
            || self.generic_structs.contains_key(name)
            || self.enum_table.contains_key(name)
    }

    /// Pass: register every `extend Type { ... }` block's methods. Extension methods are lowered
    /// exactly like struct methods (`{target}_{method}` + implicit `this`) but the target's
    /// runtime representation is untouched (it is NOT added to the struct table), so primitives
    /// keep their value/reference semantics.
    pub(super) fn register_extensions(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for ext in node.extends.iter() {
            diagnostics.file_path = file_path_string(&ext.file_path);
            let target = ext.target.text.clone();
            if ext.generic_parameters.is_some() {
                diagnostics.report_error(
                    format!(
                        "Generic 'extend' blocks are not supported yet (extending '{}')",
                        target
                    ),
                    Some(ext.target.position),
                );
                continue;
            }
            if !self.is_extendable_target(&target) {
                diagnostics.report_error(
                    format!("Cannot extend unknown type '{}'", target),
                    Some(ext.target.position),
                );
                continue;
            }
            self.register_methods_for(&target, &ext.methods, &[], diagnostics);
        }
    }

    /// Validates an `@override` object-protocol method: `@override` may only mark `to_string`
    /// / `hash_code`, those must be exported with the exact protocol signature, and a method
    /// that shadows a protocol name must carry `@override`.
    pub(super) fn validate_protocol_override(
        &self,
        method: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let name = method.name.text.as_str();

        // Constructors/destructors: `del` takes no parameters and neither declares a return type.
        if name == crate::syntax::nodes::types::DESTRUCTOR_NAME && !method.parameters.is_empty() {
            diagnostics.report_error(
                "destructor 'del' must not declare parameters".to_string(),
                Some(method.name.position),
            );
        }
        if crate::syntax::nodes::types::is_special_member_name(name) && method.return_type.is_some()
        {
            diagnostics.report_error(
                format!("'{}' must not declare a return type", name),
                Some(method.name.position),
            );
        }

        let is_protocol =
            name == crate::intrinsics::TO_STRING || name == crate::intrinsics::HASH_CODE;

        let is_override = method.attributes.iter().any(|a| a.name.text == "override");

        if is_override && !is_protocol {
            diagnostics.report_error(
                format!("'@override' can only be applied to object-protocol methods (to_string, hash_code), not '{}'", name),
                Some(method.name.position),
            );
            return;
        }

        if is_protocol && !is_override {
            diagnostics.report_error(
                format!(
                    "method '{}' overrides an object-protocol method; mark it with '@override'",
                    name
                ),
                Some(method.name.position),
            );
            return;
        }

        if is_override && is_protocol {
            if !method.is_public {
                diagnostics.report_error(
                    format!(
                        "overridden object-protocol method '{}' must be declared 'public'",
                        name
                    ),
                    Some(method.name.position),
                );
            }
            if !method.parameters.is_empty() {
                diagnostics.report_error(
                    format!(
                        "overridden object-protocol method '{}' must not declare parameters",
                        name
                    ),
                    Some(method.name.position),
                );
            }
            let return_type = method.return_type.as_ref().map(|t| t.get_type());
            let expected = if name == "to_string" { "string" } else { "int" };
            if return_type.as_deref() != Some(expected) {
                diagnostics.report_error(
                    format!("overridden '{}' must return '{}'", name, expected),
                    Some(method.name.position),
                );
            }
        }
    }
}
