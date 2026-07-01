use super::*;
use crate::diagnostics::DiagnosticBag;
use crate::semantics::errors::SemanticError;
use crate::semantics::function_table::FunctionTableInfo;
use crate::semantics::symbol_table::SymbolTable;
use crate::semantics::union_table::{
    UnionFieldInfo, UnionInfo, UnionVariantInfo, DISCRIMINANT_SIZE,
};
use crate::syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use crate::syntax::nodes::types::{
    mangle_generic, method_fn, strip_array, strip_nullable, value_size_align,
};
use crate::syntax::nodes::{EnumVariantNode, FunctionNode, ProgramNode, Type};
use crate::text::text_span::TextSpan;
use crate::syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// Pass: register every enum. A C-style integer enum (no payloads) goes into the enum table
    /// (member -> integer value). A discriminated union (any variant carries a payload) is
    /// registered as a heap reference type with a computed layout; generic unions are stashed as
    /// templates and instantiated on demand. Reports duplicate enum/member names.
    pub(super) fn register_enums(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        // Pass 1: register C-style enums and stash generic-union *templates*. Doing templates
        // first means a concrete union may reference a generic union declared later (or one from
        // the prelude, which is merged after user code), e.g. `enum Pair { Both(Option<int>) }`.
        for enum_decl in node.enums.iter() {
            let name = &enum_decl.name.text;
            if self.enum_table.contains_key(name)
                || self.union_table.contains_key(name)
                || self.generic_unions.contains_key(name)
            {
                diagnostics.report_error(
                    format!("Enum '{}' is already defined", name),
                    Some(enum_decl.name.position),
                );
                continue;
            }

            if enum_decl.is_data_enum() {
                // Generic discriminated unions are templates, monomorphized on first use.
                if enum_decl.generic_parameters.is_some() {
                    self.type_ctx
                        .register(DefKind::Union, name, generic_param_names(&enum_decl.generic_parameters));
                    self.generic_unions.insert(name.clone(), enum_decl);
                }
                continue;
            }

            // C-style integer enum: members lower to plain `i32` constants. Insertion-ordered so
            // codegen interns the variant names deterministically.
            let mut members = indexmap::IndexMap::new();
            for variant in enum_decl.variants.iter() {
                if members.contains_key(&variant.name.text) {
                    diagnostics.report_error(
                        format!(
                            "Duplicate member '{}' in enum '{}'",
                            variant.name.text, name
                        ),
                        Some(variant.name.position),
                    );
                    continue;
                }
                members.insert(variant.name.text.clone(), variant.value);
            }
            self.type_ctx.register(DefKind::Enum, name, vec![]);
            self.enum_table.insert(name.clone(), members);
        }

        // Pass 2: register concrete (non-generic) discriminated unions. Their payload fields may
        // instantiate generic unions whose templates were collected in pass 1.
        for enum_decl in node.enums.iter() {
            if enum_decl.is_data_enum() && enum_decl.generic_parameters.is_none() {
                self.register_union(&enum_decl.name.text, &enum_decl.variants, &[], diagnostics);
            }
        }
    }

    /// Computes and registers the layout of a (possibly monomorphized) discriminated union under
    /// `union_name`. Each variant's payload starts after the discriminant word; payloads of
    /// different variants overlap, so the block is sized to the largest variant. `bindings`
    /// substitutes any generic parameters in field types (empty for non-generic unions).
    pub(super) fn register_union(
        &mut self,
        union_name: &str,
        variants: &[EnumVariantNode],
        bindings: &[(String, String)],
        diagnostics: &mut DiagnosticBag,
    ) {
        let mut variant_infos = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut block_end = DISCRIMINANT_SIZE;

        for variant in variants {
            if !seen.insert(variant.name.text.clone()) {
                diagnostics.report_error(
                    format!(
                        "Duplicate variant '{}' in enum '{}'",
                        variant.name.text, union_name
                    ),
                    Some(variant.name.position),
                );
                continue;
            }
            let mut offset = DISCRIMINANT_SIZE;
            let mut field_infos = Vec::new();
            for field in &variant.fields {
                let ftype = substitute_generic_type(&field.field_type, bindings);
                // Instantiate any generic union/struct referenced by a payload field type.
                if let Some((base, args)) = Self::resolve_struct_parts(&ftype) {
                    if !args.is_empty() {
                        self.ensure_type_instantiated(
                            &base,
                            &args,
                            &field.name.position,
                            diagnostics,
                        );
                    }
                }
                let (size, align) = value_size_align(&ftype.get_type());
                let rem = offset % align;
                if rem != 0 {
                    offset += align - rem;
                }
                field_infos.push(UnionFieldInfo {
                    name: field.name.text.clone(),
                    type_: ftype,
                    offset,
                });
                offset += size;
            }
            block_end = block_end.max(offset);
            variant_infos.push(UnionVariantInfo {
                name: variant.name.text.clone(),
                discriminant: variant.value,
                fields: field_infos,
            });
        }

        // Align the block to 8 bytes so a `double` payload stays naturally aligned.
        let size = block_end.div_ceil(8) * 8;

        self.type_ctx.register(DefKind::Union, union_name, vec![]);
        if let Err(e) = self.struct_table.add_union(union_name, size, true) {
            diagnostics.report_error(e, None);
            return;
        }
        self.union_table.insert(
            union_name.to_string(),
            UnionInfo {
                name: union_name.to_string(),
                variants: variant_infos,
                size,
            },
        );
    }

    /// Ensures a generic union instantiation (e.g. `Option<int>` -> `Option_int`) is registered,
    /// monomorphizing its variant field types. No-op for non-generic or already-registered unions.
    pub(super) fn ensure_union_instantiated(
        &mut self,
        base_name: &str,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let mangled = mangle_generic(base_name, args);
        self.type_ctx
            .register_instance(DefKind::Union, base_name, args);
        if self.union_table.contains_key(&mangled) {
            return;
        }
        let template = match self.generic_unions.get(base_name) {
            Some(t) => *t,
            None => return,
        };
        let params = template.generic_parameters.as_deref().unwrap_or(&[]);
        if args.len() != params.len() {
            diagnostics.report_error(
                format!(
                    "Generic enum '{}' expects {} type argument(s), but {} were provided",
                    base_name,
                    params.len(),
                    args.len()
                ),
                Some(*position),
            );
        }
        let bindings = generic_bindings(params, args);
        self.register_union(&mangled, &template.variants, &bindings, diagnostics);
        self.register_generic_extension_methods(base_name, &mangled, args, diagnostics);
    }

    /// If a generic `extend` block targets `base_name` (e.g. `extend Option<T> { ... }`),
    /// monomorphizes its methods for the concrete instantiation `mangled` (e.g. `Option_int`),
    /// binding the extend block's own generic parameters to `args` in declaration order. A no-op
    /// when no generic extension targets `base_name`.
    pub(super) fn register_generic_extension_methods(
        &mut self,
        base_name: &str,
        mangled: &str,
        args: &[Type],
        diagnostics: &mut DiagnosticBag,
    ) {
        if let Some(ext) = self.generic_extends.get(base_name).copied() {
            let ext_params = ext.generic_parameters.as_deref().unwrap_or(&[]);
            let ext_bindings = generic_bindings(ext_params, args);
            self.register_methods_for(mangled, &ext.methods, &ext_bindings, diagnostics);
        }
    }

    /// Instantiates whichever generic container `base_name` denotes (a generic class or a generic
    /// discriminated union), so nested generic types in field/argument positions are resolved.
    pub(super) fn ensure_type_instantiated(
        &mut self,
        base_name: &str,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if self.generic_unions.contains_key(base_name) {
            self.ensure_union_instantiated(base_name, args, position, diagnostics);
        } else {
            self.ensure_struct_instantiated(base_name, args, position, diagnostics);
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
            self.type_ctx.register(
                DefKind::Struct,
                &struct_decl.name.text,
                generic_param_names(&struct_decl.generic_parameters),
            );
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
                    format!(
                        "Top-level variable '{}' is already defined",
                        global.name.text
                    ),
                    Some(global.name.position),
                );
                continue;
            }

            let gtable = self.global_symbol_table.clone();
            self.hir_global_init_begin();
            let init_type = self
                .analyze_expression(&global.initializer, &init_fn, &gtable, diagnostics)
                .unwrap_or(Type::Void);
            self.hir_global_init_finish(&global.name.text);

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
            // Register the HIR slot now (in declaration order) so a subsequent global's initializer
            // can resolve this one as a `Binding::Global`.
            self.hir_register_global(&global.name.text, &resolved.get_type(), global.is_const);
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
                self.type_ctx.register(
                    DefKind::Function,
                    &function.name.text,
                    generic_param_names(&function.generic_parameters),
                );
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
        // Register a distinct `DefId` for every non-generic function under its *emitted* name (the
        // bare base when unique, the signature-mangled key when overloaded). Deferred to here so the
        // full overload set is known: overloaded declarations must not collide on a single base def.
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                continue;
            }
            let param_types: Vec<String> = function
                .parameters
                .iter()
                .map(|p| p.type_.get_type())
                .collect();
            let emitted = self
                .function_table
                .resolve_emitted_name(&function.name.text, &param_types);
            self.type_ctx.register(DefKind::Function, &emitted, vec![]);
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
    ) -> Result<(), SemanticError> {
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
    ) -> Result<(), SemanticError> {
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
                let table =
                    self.with_generic_bindings(bindings, |s| s.analyze_function(template, diagnostics))?;
                symbol_table_map.insert(mangled_name, table);
                progressed = true;
            }

            // De-sugared struct methods, including those for newly instantiated generic structs.
            while method_index < self.struct_methods.len() {
                let (method, bindings) = self.struct_methods[method_index].clone();
                method_index += 1;
                diagnostics.file_path = file_path_string(&method.file_path);
                let table =
                    self.with_generic_bindings(bindings, |s| s.analyze_function(method, diagnostics))?;
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
        // Canonicalize the mangled bare name to the structured `(base def, args)` id so both
        // spellings of this instance lower identically.
        self.type_ctx
            .register_instance(DefKind::Struct, base_name, args);
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
        self.register_generic_extension_methods(base_name, &mangled_name, args, diagnostics);
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
        // Collect the mangled name + full parameter list (with the implicit `this`) of each method so
        // overloaded methods can be registered under their signature-mangled *emitted* names in a
        // second pass, once the whole overload set for this target is known.
        let mut registered: Vec<(String, Vec<String>)> = Vec::new();
        for method in methods {
            // Validate object-protocol overrides once (on the non-monomorphized declaration).
            if bindings.is_empty() {
                self.validate_protocol_override(method, diagnostics);
            }
            let mangled_name = method_fn(target_type_str, &method.name.text);
            self.type_ctx.register(
                DefKind::Function,
                &mangled_name,
                generic_param_names(&method.generic_parameters),
            );

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

            let param_types: Vec<String> =
                new_method.parameters.iter().map(|p| p.type_.get_type()).collect();
            let method_ref = self.arena.alloc(new_method);
            self.struct_methods.push((method_ref, bindings.to_vec()));

            if let Err(e) = self
                .function_table
                .add_overload(&mangled_name, FunctionTableInfo::from(method_ref))
            {
                diagnostics.report_error(e.to_string(), Some(method.name.position));
            }
            if method.generic_parameters.is_none() {
                registered.push((mangled_name, param_types));
            }
        }
        // Register a distinct `DefId` for each overloaded method under its emitted (signature-mangled)
        // name, so overloads don't collide on the single base-mangled def (mirrors free functions).
        for (mangled_name, param_types) in registered {
            let emitted = self
                .function_table
                .resolve_emitted_name(&mangled_name, &param_types);
            if emitted != mangled_name {
                self.type_ctx
                    .register(DefKind::Function, &emitted, vec![]);
            }
        }
    }

    /// Returns true if `name` is a type that an `extend` block may attach methods to: a
    /// primitive, `object`, a registered struct, a generic struct template, or an enum.
    pub(super) fn is_extendable_target(&self, name: &str) -> bool {
        matches!(
            name,
            "int"
                | "float"
                | "double"
                | "string"
                | "bool"
                | "char"
                | "object"
                | "JsRef"
                | "long"
                | "uint"
                | "ulong"
                | "byte"
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
                // Generic extend blocks were stashed by `stash_generic_extensions` and are attached
                // per instantiation in `ensure_*_instantiated`; here we only validate the target is
                // a known generic union or struct.
                if !self.generic_unions.contains_key(&target)
                    && !self.generic_structs.contains_key(&target)
                {
                    diagnostics.report_error(
                        format!(
                            "Cannot extend unknown generic type '{}' (no generic union or class by that name)",
                            target
                        ),
                        Some(ext.target.position),
                    );
                }
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

    /// Pre-pass: stash every generic `extend Type<...> { ... }` block keyed by its target type
    /// name, so the methods are available to monomorphize at the first instantiation of that type
    /// (which can happen as early as `register_enums`). Validation of the target is deferred to
    /// `register_extensions`, once all type templates are registered.
    pub(super) fn stash_generic_extensions(&mut self, node: &'a ProgramNode<'a>) {
        for ext in node.extends.iter() {
            if ext.generic_parameters.is_some() {
                self.generic_extends.insert(ext.target.text.clone(), ext);
            }
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
