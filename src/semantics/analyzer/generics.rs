use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::syntax::nodes::function::ParameterNode;
use crate::syntax::nodes::{FunctionNode, Type};
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::token_kind::TokenKind;

impl<'a> Analyzer<'a> {
    /// Substitutes every generic parameter appearing in a method's parameter or return types
    /// with its concrete type, according to the monomorphization bindings.
    pub(super) fn substitute_generic_signature(
        method: &mut FunctionNode<'a>,
        bindings: &[(String, String)],
    ) {
        for param in &mut method.parameters {
            param.type_ = Self::monomorphize_type(&param.type_, bindings);
        }
        if let Some(ret) = &method.return_type {
            method.return_type = Some(Self::monomorphize_type(ret, bindings));
        }
    }

    fn match_generic_type(formal: &Type, arg: &str, param_name: &str) -> Option<String> {
        match formal {
            Type::Struct(token, None) if token.text == param_name => Some(arg.to_string()),
            Type::Array(inner) => {
                if let Some(arg_inner) = arg.strip_suffix("[]") {
                    Self::match_generic_type(inner, arg_inner, param_name)
                } else {
                    None
                }
            }
            Type::Nullable(inner) => {
                if let Some(arg_inner) = arg.strip_suffix('?') {
                    Self::match_generic_type(inner, arg_inner, param_name)
                } else {
                    Self::match_generic_type(inner, arg, param_name)
                }
            }
            _ => None,
        }
    }

    /// Determines the concrete type bound to each generic parameter of `template` for one call.
    /// Uses explicit type arguments when given (arity-checked); otherwise infers each parameter
    /// from the actual argument passed to the first formal parameter that is exactly that
    /// parameter. Parameters that cannot be inferred produce a diagnostic.
    pub(super) fn infer_generic_bindings(
        &self,
        template: &FunctionNode<'a>,
        generic_args: &Option<Vec<Type>>,
        params_types: &[String],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> Vec<(String, String)> {
        let gen_params = template.generic_parameters.as_deref().unwrap_or(&[]);

        if let Some(generics) = generic_args {
            if !generics.is_empty() {
                if generics.len() != gen_params.len() {
                    diagnostics.report_error(
                        format!("Generic function '{}' expects {} type argument(s), but {} were provided", template.name.text, gen_params.len(), generics.len()),
                        Some(*position),
                    );
                }
                return gen_params
                    .iter()
                    .zip(generics.iter())
                    .map(|(param, arg)| (param.text.clone(), arg.get_type()))
                    .collect();
            }
        }

        gen_params.iter().map(|param| {
            let concrete = template.parameters.iter().enumerate().find_map(|(i, formal)| {
                params_types.get(i).and_then(|arg| {
                    Self::match_generic_type(&formal.type_, arg, &param.text)
                })
            });
            match concrete {
                Some(concrete) => (param.text.clone(), concrete),
                None => {
                    diagnostics.report_error(
                        format!("Cannot infer generic parameter '{}' of function '{}'; specify type arguments explicitly", param.text, template.name.text),
                        Some(*position),
                    );
                    (param.text.clone(), "void".to_string())
                }
            }
        }).collect()
    }

    /// Returns `ty` with any generic parameter substituted for its concrete type per the
    /// monomorphization bindings, recursing through array and nullable wrappers (`T`, `T[]`, `T?`).
    pub(super) fn monomorphize_type(ty: &Type, bindings: &[(String, String)]) -> Type {
        match ty {
            Type::Struct(token, None) => match lookup_binding(bindings, &token.text) {
                Some(concrete) => Self::concrete_type_from_str(&concrete),
                None => ty.clone(),
            },
            // A generic struct applied to type arguments (e.g. `List<T>`): substitute inside the
            // arguments so a generic function/method returning `List<T>` resolves to `List<int>`.
            Type::Struct(token, Some(args)) => Type::Struct(
                token.clone(),
                Some(
                    args.iter()
                        .map(|a| Self::monomorphize_type(a, bindings))
                        .collect(),
                ),
            ),
            Type::Array(inner) => Type::Array(Box::new(Self::monomorphize_type(inner, bindings))),
            Type::Nullable(inner) => {
                Type::Nullable(Box::new(Self::monomorphize_type(inner, bindings)))
            }
            _ => ty.clone(),
        }
    }

    /// Builds the implicit `this` parameter injected as the first argument of every method.
    /// For an extension method on a primitive, `this` is the primitive's value type (e.g.
    /// `int` -> `Type::Integer`, a stack value); for a struct it is the struct reference type.
    pub(super) fn make_this_param(struct_type_str: &str) -> ParameterNode {
        let token = synthetic_token(TokenKind::IdentifierToken, struct_type_str);
        let this_type = Type::from_token(token.clone()).unwrap_or(Type::Struct(token, None));
        ParameterNode::new(
            synthetic_token(TokenKind::IdentifierToken, "this"),
            this_type,
        )
    }
}
