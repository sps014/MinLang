use bumpalo::Bump;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, Type, ProgramNode, StatementNode};
use crate::syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use crate::syntax::nodes::function::ParameterNode;
use crate::syntax::nodes::types::{mangle_generic, mangle_with_suffixes, strip_array, strip_nullable};
use crate::syntax::syntax_tree::SyntaxTree;
use crate::syntax::text::line_text::LineText;
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::semantics::function_control_flow::FunctionControlGraph;
use crate::semantics::function_table::{FunctionTable, FunctionTableInfo};
use crate::semantics::symbol_table::SymbolTable;
use crate::semantics::struct_table::StructTable;
use crate::driver::diagnostics::DiagnosticBag;
use super::*;

impl<'a> Analyzer<'a> {
    /// Pass: register every enum and its members (member -> integer value), reporting duplicate
    /// enum names and duplicate member names.
    pub(super) fn register_enums(&mut self, node: &'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) {
        for enum_decl in node.enums.iter() {
            if self.enum_table.contains_key(&enum_decl.name.text) {
                diagnostics.report_error(
                    format!("Enum '{}' is already defined", enum_decl.name.text),
                    Some(enum_decl.name.position.clone()),
                );
                continue;
            }
            let mut members = HashMap::new();
            for (member, value) in enum_decl.members.iter() {
                if members.contains_key(&member.text) {
                    diagnostics.report_error(
                        format!("Duplicate member '{}' in enum '{}'", member.text, enum_decl.name.text),
                        Some(member.position.clone()),
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
        self.enum_table.get(enum_name).and_then(|m| m.get(member)).copied()
    }

    /// Enum-typed values are integers at runtime, so an enum type and `int` are mutually
    /// assignable/comparable (C-style). Used to relax type checks involving enums.
    pub(super) fn enum_int_compatible(&self, a: &str, b: &str) -> bool {
        (self.enum_table.contains_key(a) && b == "int")
            || (self.enum_table.contains_key(b) && a == "int")
    }

    /// Pass 0: register every (non-generic) struct and its methods; stash generic templates.
    pub(super) fn register_structs(&mut self, node: &'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) {
        for struct_decl in node.structs.iter() {
            diagnostics.file_path = file_path_string(&struct_decl.file_path);
            if struct_decl.generic_parameters.is_some() {
                self.generic_structs.insert(struct_decl.name.text.clone(), struct_decl);
                continue;
            }
            if let Err(e) = self.struct_table.add_struct(struct_decl) {
                diagnostics.report_error(e, Some(struct_decl.name.position.clone()));
            }
            self.register_struct_methods(struct_decl, &struct_decl.name.text, &[], diagnostics);
        }
    }

    /// Pass 1: register every (non-generic) function signature; stash generic templates.
    pub(super) fn register_functions(&mut self, node: &'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) {
        for function in node.functions.iter() {
            diagnostics.file_path = file_path_string(&function.file_path);
            self.check_reserved_name(&function.name, "function", diagnostics);
            if function.generic_parameters.is_some() {
                self.generic_functions.insert(function.name.text.clone(), function);
                continue;
            }
            if function.is_exported {
                self.check_export_visibility(function, diagnostics);
            }
            if let Err(e) = self.function_table.add_function(function.name.text.clone(), FunctionTableInfo::from(function)) {
                diagnostics.report_error(e.to_string(), Some(function.name.position.clone()));
            }
        }
    }

    /// Ensures an exported function does not leak a non-exported struct through its signature.
    pub(super) fn check_export_visibility(&self, function: &FunctionNode<'a>, diagnostics: &mut DiagnosticBag) {
        let signature_types = function.return_type.iter()
            .chain(function.parameters.iter().map(|p| &p.type_));
        for type_to_check in signature_types {
            let base_type_str = strip_nullable(strip_array(&type_to_check.get_type())).to_string();
            if let Some(struct_info) = self.struct_table.get_struct(&base_type_str) {
                if !struct_info.is_exported {
                    diagnostics.report_error(
                        format!("Exported function '{}' exposes unexported struct '{}'", function.name.text, base_type_str),
                        Some(function.name.position.clone()),
                    );
                }
            }
        }
    }

    /// Pass 2: analyze the body of every concrete function.
    pub(super) fn analyze_function_bodies(&mut self, node: &'a ProgramNode<'a>, symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
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
            symbol_table_map.insert(function.name.text.clone(), table);
        }
        Ok(())
    }

    /// Pass 3: analyze each monomorphized generic instance so concrete-type errors surface.
    pub(super) fn analyze_instantiated_generics(&mut self, symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let generics_to_analyze: Vec<(String, &'a FunctionNode<'a>)> = self.instantiated_generics.iter()
            .map(|(mangled, (_bindings, template))| (mangled.clone(), *template))
            .collect();
        let bindings_by_name: HashMap<String, GenericBindings> = self.instantiated_generics.iter()
            .map(|(mangled, (bindings, _))| (mangled.clone(), bindings.clone()))
            .collect();
        for (mangled_name, template) in generics_to_analyze {
            diagnostics.file_path = file_path_string(&template.file_path);
            self.current_generic_bindings = bindings_by_name.get(&mangled_name).cloned().unwrap_or_default();
            let table = self.analyze_function(template, diagnostics)?;
            self.current_generic_bindings = Vec::new();
            symbol_table_map.insert(mangled_name, table);
        }
        Ok(())
    }

    /// Pass 4: analyze the body of every (de-sugared) struct method.
    pub(super) fn analyze_struct_method_bodies(&mut self, symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let methods_to_analyze = self.struct_methods.clone();
        for (method, bindings) in methods_to_analyze {
            diagnostics.file_path = file_path_string(&method.file_path);
            self.current_generic_bindings = bindings;
            let table = self.analyze_function(method, diagnostics)?;
            self.current_generic_bindings = Vec::new();
            symbol_table_map.insert(method.name.text.clone(), table);
        }
        Ok(())
    }
    pub(super) fn ensure_struct_instantiated(&mut self, base_name: &str, args: &[Type], position: &TextSpan, diagnostics: &mut DiagnosticBag) {
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
                format!("Generic struct '{}' expects {} type argument(s), but {} were provided", base_name, params.len(), args.len()),
                Some(position.clone()),
            );
        }
        let bindings = generic_bindings(params, args);

        let new_fields = template.fields.iter()
            .map(|field| StructFieldNode {
                name: field.name.clone(),
                type_token: substitute_generic_token(&field.type_token, &bindings),
            })
            .collect();

        let mut new_name_token = template.name.clone();
        new_name_token.text = mangled_name.clone();
        let new_decl = StructDeclarationNode::new(
            new_name_token,
            None,
            new_fields,
            template.methods.clone(),
            template.is_exported,
        );

        if let Err(e) = self.struct_table.add_struct(&new_decl) {
            diagnostics.report_error(e, Some(position.clone()));
        }

        self.register_struct_methods(&new_decl, &mangled_name, &bindings, diagnostics);
    }

    pub(super) fn register_struct_methods(&mut self, struct_decl: &StructDeclarationNode<'a>, struct_type_str: &str, bindings: &[(String, String)], diagnostics: &mut DiagnosticBag) {
        for method in &struct_decl.methods {
            // Validate object-protocol overrides once (on the non-monomorphized declaration).
            if bindings.is_empty() {
                self.validate_protocol_override(method, diagnostics);
            }
            let mangled_name = format!("{}_{}", struct_type_str, method.name.text);

            let mut new_method = method.clone();
            new_method.name = synthetic_token(TokenKind::IdentifierToken, &mangled_name);

            if !bindings.is_empty() {
                Self::substitute_generic_signature(&mut new_method, bindings);
            }

            new_method.parameters.insert(0, Self::make_this_param(struct_type_str));

            let method_ref = self.arena.alloc(new_method);
            self.struct_methods.push((method_ref, bindings.to_vec()));

            if let Err(e) = self.function_table.add_function(mangled_name.clone(), FunctionTableInfo::from(method_ref)) {
                diagnostics.report_error(e.to_string(), Some(method.name.position.clone()));
            }
        }
    }

    /// Validates an `@override` object-protocol method: `@override` may only mark `to_string`
    /// / `hash_code`, those must be exported with the exact protocol signature, and a method
    /// that shadows a protocol name must carry `@override`.
    pub(super) fn validate_protocol_override(&self, method: &FunctionNode<'a>, diagnostics: &mut DiagnosticBag) {
        let name = method.name.text.as_str();

        // Constructors/destructors: `drop` takes no parameters and neither declares a return type.
        if name == "drop" && !method.parameters.is_empty() {
            diagnostics.report_error(
                "destructor 'drop' must not declare parameters".to_string(),
                Some(method.name.position.clone()),
            );
        }
        if (name == "init" || name == "drop") && method.return_type.is_some() {
            diagnostics.report_error(
                format!("'{}' must not declare a return type", name),
                Some(method.name.position.clone()),
            );
        }

        let is_protocol = name == "to_string" || name == "hash_code";

        if method.is_override && !is_protocol {
            diagnostics.report_error(
                format!("'@override' can only be applied to object-protocol methods (to_string, hash_code), not '{}'", name),
                Some(method.name.position.clone()),
            );
            return;
        }

        if is_protocol && !method.is_override {
            diagnostics.report_error(
                format!("method '{}' overrides an object-protocol method; mark it with '@override'", name),
                Some(method.name.position.clone()),
            );
            return;
        }

        if method.is_override && is_protocol {
            if !method.is_exported {
                diagnostics.report_error(
                    format!("overridden object-protocol method '{}' must be declared 'pub'", name),
                    Some(method.name.position.clone()),
                );
            }
            if !method.parameters.is_empty() {
                diagnostics.report_error(
                    format!("overridden object-protocol method '{}' must not declare parameters", name),
                    Some(method.name.position.clone()),
                );
            }
            let return_type = method.return_type.as_ref().map(|t| t.get_type());
            let expected = if name == "to_string" { "string" } else { "int" };
            if return_type.as_deref() != Some(expected) {
                diagnostics.report_error(
                    format!("overridden '{}' must return '{}'", name, expected),
                    Some(method.name.position.clone()),
                );
            }
        }
    }
}
