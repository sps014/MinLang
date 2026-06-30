//! Analysis of discriminated-union construction (`Enum.Variant(args)` / unit `Enum.Variant`) and
//! of `match` expressions/statements: pattern typing, binding scopes, guards, arm-type
//! unification, exhaustiveness, and unreachable-arm detection.

use super::*;
use crate::driver::diagnostics::DiagnosticBag;
use crate::semantics::errors::SemanticError;
use crate::semantics::symbol_table::SymbolTable;
use crate::semantics::union_table::UnionInfo;
use crate::syntax::nodes::types::strip_nullable;
use crate::syntax::nodes::{
    ExpressionNode, FunctionNode, MatchArm, MatchArmBody, PatternNode, Type,
};
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// What checking a single pattern told us, used to drive exhaustiveness and unreachable-arm
/// analysis.
struct PatternInfo {
    /// True when the pattern matches every value of its type (a bare binding or `_`).
    irrefutable: bool,
    /// The union variant fully covered by this pattern (all sub-patterns irrefutable), if any.
    covered_variant: Option<String>,
}

impl<'a> Analyzer<'a> {
    /// If `enum_name` denotes a discriminated union (concrete or generic) and `variant` names one
    /// of its variants, type-checks the construction `Enum.Variant(args)` and returns its type.
    /// Returns `Ok(None)` when `enum_name` is not a union (so the caller can fall through to its
    /// normal handling, e.g. C-style enum member access or a static method call).
    pub(super) fn analyze_variant_construction(
        &mut self,
        enum_name: &str,
        variant: &SyntaxToken,
        args: &[ExpressionNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<Type>, SemanticError> {
        let is_generic = self.generic_unions.contains_key(enum_name);
        let is_concrete = self.union_table.contains_key(enum_name);
        if !is_generic && !is_concrete {
            return Ok(None);
        }

        // The declared payload field types of the named variant (templated for generic unions).
        let field_types: Vec<Type> = if is_generic {
            let template = *self.generic_unions.get(enum_name).unwrap();
            match template
                .variants
                .iter()
                .find(|v| v.name.text == variant.text)
            {
                Some(v) => v.fields.iter().map(|f| f.field_type.clone()).collect(),
                None => {
                    return Err(report(
                        diagnostics,
                        format!("Enum '{}' has no variant '{}'", enum_name, variant.text),
                        Some(variant.position),
                    ));
                }
            }
        } else {
            let info = self.union_table.get(enum_name).unwrap();
            match info.variant(&variant.text) {
                Some(v) => v.fields.iter().map(|f| f.type_.clone()).collect(),
                None => {
                    return Err(report(
                        diagnostics,
                        format!("Enum '{}' has no variant '{}'", enum_name, variant.text),
                        Some(variant.position),
                    ));
                }
            }
        };

        let mut arg_types = Vec::new();
        for arg in args {
            arg_types.push(self.analyze_expression(
                arg,
                parent_function,
                symbol_table,
                diagnostics,
            )?);
        }

        if args.len() != field_types.len() {
            diagnostics.report_error(
                format!(
                    "Variant '{}.{}' expects {} argument(s), but {} were given",
                    enum_name,
                    variant.text,
                    field_types.len(),
                    args.len()
                ),
                Some(variant.position),
            );
        }

        if !is_generic {
            for (i, ft) in field_types.iter().enumerate() {
                if let Some(at) = arg_types.get(i) {
                    if !self.type_str_assignable(&ft.get_type(), &at.get_type()) {
                        diagnostics.report_error(
                            format!(
                                "Variant '{}.{}' expects argument {} to be '{}', got '{}'",
                                enum_name,
                                variant.text,
                                i + 1,
                                ft.get_type(),
                                at.get_type()
                            ),
                            Some(variant.position),
                        );
                    }
                }
            }
            return Ok(Some(Type::Struct(
                synthetic_token(TokenKind::IdentifierToken, enum_name),
                None,
            )));
        }

        // Generic union: resolve the concrete type arguments, preferring an explicit expected type
        // (e.g. a `let`/`return` annotation) and otherwise inferring from the arguments.
        let template = *self.generic_unions.get(enum_name).unwrap();
        let params: Vec<String> = template
            .generic_parameters
            .as_ref()
            .map(|ps| ps.iter().map(|p| p.text.clone()).collect())
            .unwrap_or_default();

        let mut concrete_args: Option<Vec<Type>> = None;
        if let Some(Type::Struct(b, Some(eargs))) = &self.current_expected_type {
            if b.text == enum_name && eargs.len() == params.len() {
                concrete_args = Some(eargs.clone());
            }
        }
        if concrete_args.is_none() {
            let mut binding: HashMap<String, Type> = HashMap::new();
            for (ft, at) in field_types.iter().zip(arg_types.iter()) {
                let name = ft.get_type();
                if params.contains(&name) {
                    binding.entry(name).or_insert_with(|| at.clone());
                }
            }
            let resolved: Vec<Type> = params
                .iter()
                .filter_map(|p| binding.get(p).cloned())
                .collect();
            if resolved.len() == params.len() {
                concrete_args = Some(resolved);
            }
        }

        let concrete_args = match concrete_args {
            Some(a) => a,
            None => {
                return Err(report(
                    diagnostics,
                    format!(
                        "Cannot infer type arguments for '{}.{}'; add a type annotation (e.g. `let x: {}<...> = ...`)",
                        enum_name, variant.text, enum_name
                    ),
                    Some(variant.position),
                ));
            }
        };

        let bindings = generic_bindings(
            template.generic_parameters.as_deref().unwrap_or(&[]),
            &concrete_args,
        );
        for (i, ft) in field_types.iter().enumerate() {
            let expected = substitute_generic_type(ft, &bindings);
            if let Some(at) = arg_types.get(i) {
                if !self.type_str_assignable(&expected.get_type(), &at.get_type()) {
                    diagnostics.report_error(
                        format!(
                            "Variant '{}.{}' expects argument {} to be '{}', got '{}'",
                            enum_name,
                            variant.text,
                            i + 1,
                            expected.get_type(),
                            at.get_type()
                        ),
                        Some(variant.position),
                    );
                }
            }
        }

        self.ensure_union_instantiated(enum_name, &concrete_args, &variant.position, diagnostics);
        Ok(Some(Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, enum_name),
            Some(concrete_args),
        )))
    }

    /// Analyzes a `match`. `is_expression` is true when the match is used in value position (all
    /// arms must be `=> expr` and share one type); false in statement position (block arms are
    /// allowed and the result is `void`). Returns the unified arm type (or `void`).
    pub(super) fn analyze_match(
        &mut self,
        subject: &ExpressionNode<'a>,
        arms: &[MatchArm<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        is_expression: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let subject_type =
            self.analyze_expression(subject, parent_function, symbol_table, diagnostics)?;
        // The subject's union may be a generic instantiation that has not been constructed yet
        // (e.g. matching on a `param: Option<int>`); ensure its layout is registered first.
        if let Type::Struct(base, Some(args)) = &subject_type {
            if self.generic_unions.contains_key(&base.text) {
                self.ensure_union_instantiated(&base.text, args, &base.position, diagnostics);
            }
        }
        let subject_base = strip_nullable(&subject_type.get_type()).to_string();
        let union_info: Option<UnionInfo> = self.union_table.get(&subject_base).cloned();

        let mut arm_value_type: Option<Type> = None;
        let mut covered: HashSet<String> = HashSet::new();
        let mut has_catch_all = false;
        let mut catch_all_index: Option<usize> = None;

        for (i, arm) in arms.iter().enumerate() {
            if catch_all_index.is_some() {
                diagnostics.report_error(
                    "Unreachable match arm: a previous arm already matches everything".to_string(),
                    arm.pattern.position(),
                );
            }

            // Each arm introduces its pattern bindings into a fresh child scope.
            let arm_scope = Rc::new(RefCell::new(SymbolTable::new(Some(symbol_table.clone()))));
            (*symbol_table).borrow_mut().add_child(arm_scope.clone());

            let info = self.check_pattern(&arm.pattern, &subject_type, &arm_scope, diagnostics)?;

            if let Some(guard) = &arm.guard {
                let gt =
                    self.analyze_expression(guard, parent_function, &arm_scope, diagnostics)?;
                if !gt.is_unknown() && gt.get_type() != "bool" {
                    diagnostics.report_error(
                        format!("match guard must be a bool, got {}", gt.get_type()),
                        guard.position(),
                    );
                }
            }

            match &arm.body {
                MatchArmBody::Expr(expr) => {
                    let t =
                        self.analyze_expression(expr, parent_function, &arm_scope, diagnostics)?;
                    if is_expression {
                        match &arm_value_type {
                            None => arm_value_type = Some(t),
                            Some(prev) => {
                                self.compare_data_type(prev, &t, &empty_span(), diagnostics)?
                            }
                        }
                    }
                }
                MatchArmBody::Block(stmts) => {
                    if is_expression {
                        diagnostics.report_error(
                            "A block arm (`=> { ... }`) is only allowed when `match` is used as a statement; use `=> expr` in expression position".to_string(),
                            arm.pattern.position(),
                        );
                    }
                    self.analyze_body(
                        stmts,
                        parent_function,
                        Some(&arm_scope),
                        false,
                        diagnostics,
                    )?;
                }
            }

            // An arm only contributes to exhaustiveness when it has no guard (a guard may fail).
            if arm.guard.is_none() {
                if info.irrefutable {
                    has_catch_all = true;
                    catch_all_index = Some(i);
                } else if let Some(v) = info.covered_variant {
                    covered.insert(v);
                }
            }
        }

        // Exhaustiveness: every union variant must be covered, or a catch-all arm present.
        if !has_catch_all {
            if let Some(info) = &union_info {
                let missing: Vec<String> = info
                    .variants
                    .iter()
                    .filter(|v| !covered.contains(&v.name))
                    .map(|v| v.name.clone())
                    .collect();
                if !missing.is_empty() {
                    diagnostics.report_error(
                        format!(
                            "Non-exhaustive match on '{}': missing variant(s) {}. Add the missing arm(s) or a `_` arm",
                            subject_base,
                            missing.join(", ")
                        ),
                        subject.position(),
                    );
                }
            } else if !subject_type.is_unknown() {
                diagnostics.report_error(
                    format!(
                        "Non-exhaustive match on '{}': add a `_` arm to cover all cases",
                        subject_base
                    ),
                    subject.position(),
                );
            }
        }

        if is_expression {
            Ok(arm_value_type.unwrap_or(Type::Void))
        } else {
            Ok(Type::Void)
        }
    }

    /// Type-checks `pattern` against `expected`, introducing any bindings into `scope`.
    fn check_pattern(
        &mut self,
        pattern: &PatternNode,
        expected: &Type,
        scope: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<PatternInfo, SemanticError> {
        let expected_base = strip_nullable(&expected.get_type()).to_string();
        let union_info: Option<UnionInfo> = self.union_table.get(&expected_base).cloned();

        match pattern {
            PatternNode::Wildcard(_) => Ok(PatternInfo {
                irrefutable: true,
                covered_variant: None,
            }),
            PatternNode::Binding(name) => {
                // A bare identifier that names a unit variant of the matched union is a
                // unit-variant pattern; otherwise it binds the whole value.
                if let Some(info) = &union_info {
                    if let Some(v) = info.variant(&name.text) {
                        if v.fields.is_empty() {
                            return Ok(PatternInfo {
                                irrefutable: false,
                                covered_variant: Some(name.text.clone()),
                            });
                        }
                    }
                }
                if let Err(e) = (*scope)
                    .borrow_mut()
                    .add_symbol(name.text.clone(), expected.clone())
                {
                    diagnostics.report_error(e.to_string(), Some(name.position));
                }
                Ok(PatternInfo {
                    irrefutable: true,
                    covered_variant: None,
                })
            }
            PatternNode::Literal(lit) => {
                if !lit.is_unknown()
                    && !expected.is_unknown()
                    && !self.type_str_assignable(&expected_base, &lit.get_type())
                {
                    diagnostics.report_error(
                        format!(
                            "Pattern literal of type '{}' cannot match a value of type '{}'",
                            lit.get_type(),
                            expected_base
                        ),
                        lit.get_span(),
                    );
                }
                Ok(PatternInfo {
                    irrefutable: false,
                    covered_variant: None,
                })
            }
            PatternNode::Variant(qualifier, variant, subs) => {
                let info = match &union_info {
                    Some(info) => info.clone(),
                    None => {
                        diagnostics.report_error(
                            format!(
                                "Variant pattern '{}' can only match a discriminated union, not '{}'",
                                variant.text, expected_base
                            ),
                            Some(variant.position),
                        );
                        // Still walk sub-patterns so their bindings/errors surface.
                        for sub in subs {
                            self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                        }
                        return Ok(PatternInfo {
                            irrefutable: false,
                            covered_variant: None,
                        });
                    }
                };

                if let Some(q) = qualifier {
                    if q.text != expected_base {
                        diagnostics.report_error(
                            format!(
                                "Variant qualifier '{}' does not match the matched enum '{}'",
                                q.text, expected_base
                            ),
                            Some(q.position),
                        );
                    }
                }

                let var_info = match info.variant(&variant.text) {
                    Some(v) => v.clone(),
                    None => {
                        diagnostics.report_error(
                            format!("Enum '{}' has no variant '{}'", expected_base, variant.text),
                            Some(variant.position),
                        );
                        for sub in subs {
                            self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                        }
                        return Ok(PatternInfo {
                            irrefutable: false,
                            covered_variant: None,
                        });
                    }
                };

                if subs.len() != var_info.fields.len() {
                    diagnostics.report_error(
                        format!(
                            "Variant '{}.{}' has {} field(s), but the pattern binds {}",
                            expected_base,
                            variant.text,
                            var_info.fields.len(),
                            subs.len()
                        ),
                        Some(variant.position),
                    );
                }

                let mut all_irrefutable = true;
                for (i, sub) in subs.iter().enumerate() {
                    let field_type = var_info
                        .fields
                        .get(i)
                        .map(|f| f.type_.clone())
                        .unwrap_or(Type::Unknown);
                    let sub_info = self.check_pattern(sub, &field_type, scope, diagnostics)?;
                    if !sub_info.irrefutable {
                        all_irrefutable = false;
                    }
                }

                // The variant is fully covered only when every sub-pattern is irrefutable.
                Ok(PatternInfo {
                    irrefutable: false,
                    covered_variant: if all_irrefutable {
                        Some(variant.text.clone())
                    } else {
                        None
                    },
                })
            }
        }
    }
}
