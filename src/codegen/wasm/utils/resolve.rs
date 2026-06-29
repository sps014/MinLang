//! Name resolution for codegen: mapping call/method/static-call syntax to the concrete, mangled
//! function names that were emitted (including overload selection and constructor mangling).

use super::super::WasmGenerator;
use crate::semantics::function_table::{overload_arg_compatible, OverloadResolution};
use crate::syntax::nodes::types::{canonical_type_name, mangle_generic, method_fn};
use crate::syntax::nodes::{FunctionNode, Type};

impl<'a> WasmGenerator<'a> {
    /// Resolves a (possibly generic) function call to its concrete, mangled name.
    /// Uses explicit generic arguments when present, otherwise infers the type from the
    /// first argument and falls back to the plain name when no monomorphized variant exists.
    pub fn resolve_call_name(
        &self,
        name: &str,
        generic_args: &Option<Vec<Type>>,
        args: &[crate::syntax::nodes::ExpressionNode<'a>],
        function: &FunctionNode<'a>,
    ) -> String {
        if let Some(generics) = generic_args {
            if !generics.is_empty() {
                return mangle_generic(name, generics);
            }
        }
        // Overloaded free functions: pick the emitted variant whose signature matches the argument
        // types, mirroring the analyzer's selection so both agree on the callee.
        if self.function_table.is_overloaded(name) {
            let arg_types: Vec<String> = args
                .iter()
                .map(|arg| {
                    self.infer_expression_type(arg, function)
                        .unwrap_or_default()
                })
                .collect();
            let compat = |param: &str, arg: &str| {
                overload_arg_compatible(param, arg, |t| self.enums.contains_key(t))
            };
            if let OverloadResolution::Unique(key) = self
                .function_table
                .select_overload(name, &arg_types, compat)
            {
                return key;
            }
        }
        name.to_string()
    }

    /// Resolves a method call `obj.method(params)` to the emitted function name, selecting among
    /// overloads by argument types (the receiver is supplied as the implicit `this` argument).
    /// Returns the bare `{struct}_{method}` base when the method is not overloaded.
    pub fn resolve_method_key(
        &self,
        struct_name: &str,
        method: &str,
        params: &[crate::syntax::nodes::ExpressionNode<'a>],
        function: &FunctionNode<'a>,
    ) -> String {
        let base = method_fn(struct_name, method);
        if !self.function_table.is_overloaded(&base) {
            return base;
        }
        let mut arg_types = Vec::with_capacity(params.len() + 1);
        arg_types.push(struct_name.to_string());
        for param in params {
            arg_types.push(
                self.infer_expression_type(param, function)
                    .unwrap_or_default(),
            );
        }
        let compat =
            |p: &str, a: &str| overload_arg_compatible(p, a, |t| self.enums.contains_key(t));
        match self
            .function_table
            .select_overload(&base, &arg_types, compat)
        {
            OverloadResolution::Unique(key) => key,
            _ => base,
        }
    }

    /// True if `name` is a local variable/parameter of the function currently being emitted.
    pub fn is_local_var(&self, name: &str, function: &FunctionNode<'a>) -> bool {
        let func_name = self
            .ctx
            .current_mangled_name
            .as_ref()
            .unwrap_or(&function.name.text);
        self.ctx
            .combined_symbol_lookup
            .get(func_name)
            .map(|m| m.contains_key(name))
            .unwrap_or(false)
    }

    /// If `obj.method(...)` is actually a static call `Type.method(...)` (the receiver names a type,
    /// not a local value, and `{type}_{method}` exists), returns the emitted function name to call
    /// (overload-resolved over the explicit arguments, which carry no implicit `this`).
    pub fn resolve_static_call(
        &self,
        obj: &crate::syntax::nodes::ExpressionNode<'a>,
        method: &str,
        params: &[crate::syntax::nodes::ExpressionNode<'a>],
        function: &FunctionNode<'a>,
    ) -> Option<String> {
        let crate::syntax::nodes::ExpressionNode::Identifier(id) = obj else {
            return None;
        };
        if self.is_local_var(&id.text, function) {
            return None;
        }
        let type_name = canonical_type_name(&id.text)
            .unwrap_or(id.text.as_str())
            .to_string();
        let base = method_fn(&type_name, method);
        if !(self.function_table.is_overloaded(&base)
            || self.function_table.get_function(&base).is_ok())
        {
            return None;
        }
        Some(self.resolve_call_name(&base, &None, params, function))
    }

    /// The monomorphized struct name a constructor call `Name(...)` / `Name<T>(...)` targets,
    /// mirroring the mangling used by struct instantiation (e.g. `Point<int>` -> `Point_int`).
    pub fn constructor_struct_name(&self, name: &str, generic_args: &Option<Vec<Type>>) -> String {
        match generic_args {
            Some(args) if !args.is_empty() => {
                let mut mangled = name.to_string();
                for arg in args {
                    mangled.push('_');
                    mangled.push_str(&self.resolve_type(&arg.get_type()));
                }
                mangled
            }
            _ => name.to_string(),
        }
    }
}
