//! Analysis of discriminated-union construction (`Enum.Variant(args)` / unit `Enum.Variant`) and
//! of `match` expressions/statements: pattern typing, binding scopes, guards, arm-type
//! unification, exhaustiveness, and unreachable-arm detection.

use super::*;
use crate::diagnostics::DiagnosticBag;
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

/// The HIR shape a match pattern lowers to (for statement-position `match` → [`HStmt::Switch`]).
enum HirArmShape {
    /// A `Const` arm (literal pattern).
    Const(crate::hir::HExpr),
    /// A `Variant` arm; `bindings` are the payload local slots in field order.
    Variant {
        def: crate::types::DefId,
        variant: usize,
        bindings: Vec<crate::hir::LocalId>,
    },
    /// A catch-all (`_` or a bare binding that names no variant) → the switch `default` block.
    Default,
    /// Not representable in HIR's `Switch` (nested sub-pattern, bind-whole-value, bad literal).
    Unsupported,
}

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
        let mut arg_hirs = Vec::new();
        for arg in args {
            let t = self.analyze_expression(arg, parent_function, symbol_table, diagnostics)?;
            arg_hirs.push(self.hir_take());
            arg_types.push(t);
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
            let result_ty =
                Type::Struct(synthetic_token(TokenKind::IdentifierToken, enum_name), None);
            // Construct the union value: resolve its `DefId` and the variant's discriminant.
            let def = self.type_ctx.defs.lookup(crate::types::DefKind::Union, enum_name);
            let disc = self
                .union_table
                .get(enum_name)
                .and_then(|i| i.variant(&variant.text))
                .map(|v| v.discriminant as usize);
            match (def, disc) {
                (Some(def), Some(disc)) => {
                    self.hir_set_union_new(def, disc, arg_hirs, &result_ty)
                }
                _ => self.hir_none(),
            }
            return Ok(Some(result_ty));
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
        // Generic union construction needs an `InstanceId` (a later slice); drop out of coverage.
        self.hir_none();
        Ok(Some(Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, enum_name),
            Some(concrete_args),
        )))
    }

    /// Analyzes a `match`. `is_expression` is true when the match is used in value position (all
    /// Classifies a match pattern for HIR statement-`match` lowering, allocating HIR locals for any
    /// variant-payload bindings *before* the arm body is lowered so the body can resolve them.
    fn hir_match_pattern(
        &mut self,
        pattern: &PatternNode,
        union_info: &Option<UnionInfo>,
        union_def: Option<crate::types::DefId>,
    ) -> HirArmShape {
        match pattern {
            PatternNode::Wildcard(_) => HirArmShape::Default,
            PatternNode::Binding(name) => {
                // A bare identifier naming a unit variant is a unit-variant pattern; otherwise it
                // binds the whole subject, which HIR's `Switch` cannot express.
                if let (Some(info), Some(def)) = (union_info, union_def) {
                    if let Some(v) = info.variant(&name.text) {
                        if v.fields.is_empty() {
                            return HirArmShape::Variant {
                                def,
                                variant: v.discriminant as usize,
                                bindings: vec![],
                            };
                        }
                    }
                }
                HirArmShape::Unsupported
            }
            PatternNode::Literal(lit) => {
                self.hir_set_literal(lit);
                match self.hir_take() {
                    Some(e) => HirArmShape::Const(e),
                    None => HirArmShape::Unsupported,
                }
            }
            PatternNode::Variant(_, name, subs) => {
                let (Some(info), Some(def)) = (union_info, union_def) else {
                    return HirArmShape::Unsupported;
                };
                let Some(v) = info.variant(&name.text) else {
                    return HirArmShape::Unsupported;
                };
                if subs.len() != v.fields.len() {
                    return HirArmShape::Unsupported;
                }
                let fields: Vec<(String, Type)> = v
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.type_.clone()))
                    .collect();
                let variant = v.discriminant as usize;
                let mut bindings = Vec::with_capacity(subs.len());
                for (i, sub) in subs.iter().enumerate() {
                    // Only flat `Binding`/`_` sub-patterns are representable; each field gets a slot.
                    let (slot_name, fty) = match sub {
                        PatternNode::Binding(bn) => (bn.text.clone(), fields[i].1.clone()),
                        PatternNode::Wildcard(_) => {
                            (format!("__match_{}_{}", variant, i), fields[i].1.clone())
                        }
                        _ => return HirArmShape::Unsupported,
                    };
                    match self.hir_alloc_local(&slot_name, &fty) {
                        Some(id) => bindings.push(id),
                        None => return HirArmShape::Unsupported,
                    }
                }
                HirArmShape::Variant {
                    def,
                    variant,
                    bindings,
                }
            }
        }
    }

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
        let subject_hir = self.hir_take();
        // The subject's union may be a generic instantiation that has not been constructed yet
        // (e.g. matching on a `param: Option<int>`); ensure its layout is registered first.
        if let Type::Struct(base, Some(args)) = &subject_type {
            if self.generic_unions.contains_key(&base.text) {
                self.ensure_union_instantiated(&base.text, args, &base.position, diagnostics);
            }
        }
        let subject_base = strip_nullable(&subject_type.get_type()).to_string();
        let union_info: Option<UnionInfo> = self.union_table.get(&subject_base).cloned();
        let union_def = self
            .type_ctx
            .defs
            .lookup(crate::types::DefKind::Union, &subject_base);

        let mut arm_value_type: Option<Type> = None;
        let mut covered: HashSet<String> = HashSet::new();
        let mut has_catch_all = false;
        let mut catch_all_index: Option<usize> = None;

        // HIR: build `Switch` arms + a default block. A statement-position match lowers directly; a
        // value-position match desugars to `<result temp> = arm; … ; <result temp read>`, with each
        // arm body assigning the shared result temporary.
        let mut hir_arms: Vec<crate::hir::HArm> = Vec::new();
        let mut hir_default: Vec<crate::hir::HStmt> = Vec::new();
        let mut hir_ok = subject_hir.is_some();
        let mut result_temp: Option<crate::hir::LocalId> = None;
        let mut result_ty_id: Option<crate::types::TypeId> = None;

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
                // HIR `Switch` arms have no guard, so a guarded arm drops the function out of coverage.
                hir_ok = false;
                let gt =
                    self.analyze_expression(guard, parent_function, &arm_scope, diagnostics)?;
                if !gt.is_unknown() && gt.get_type() != "bool" {
                    diagnostics.report_error(
                        format!("match guard must be a bool, got {}", gt.get_type()),
                        guard.position(),
                    );
                }
            }

            // Classify the pattern (allocating payload binding slots) before the body is lowered.
            let shape = self.hir_match_pattern(&arm.pattern, &union_info, union_def);

            self.hir_open_block();
            match &arm.body {
                MatchArmBody::Expr(expr) => {
                    let t =
                        self.analyze_expression(expr, parent_function, &arm_scope, diagnostics)?;
                    let arm_hir = self.hir_take();
                    if is_expression {
                        match &arm_value_type {
                            None => arm_value_type = Some(t.clone()),
                            Some(prev) => {
                                self.compare_data_type(prev, &t, &empty_span(), diagnostics)?
                            }
                        }
                        // Desugar: assign the arm value to the shared result temp (allocated from the
                        // first arm's type), so the whole match reads back as one value.
                        if result_temp.is_none() {
                            result_temp = self.hir_alloc_local("__match_result", &t);
                            result_ty_id = Some(self.type_ctx.lower(&t));
                        }
                        match result_temp {
                            Some(tmp) => self.hir_assign_local_id(tmp, arm_hir),
                            None => hir_ok = false,
                        }
                    } else {
                        self.hir_expr_stmt(arm_hir);
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
            let body_hir = self.hir_close_block();

            match shape {
                HirArmShape::Default => hir_default = body_hir,
                HirArmShape::Const(label) => match self.hir_const_arm(Some(label), body_hir) {
                    Some(arm) => hir_arms.push(arm),
                    None => hir_ok = false,
                },
                HirArmShape::Variant {
                    def,
                    variant,
                    bindings,
                } => hir_arms.push(self.hir_variant_arm(def, variant, bindings, body_hir)),
                HirArmShape::Unsupported => hir_ok = false,
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

        if is_expression {
            // Emit the desugared switch, then leave the result temp read as the match's value.
            match (result_temp, result_ty_id) {
                (Some(tmp), Some(ty)) if hir_ok => {
                    self.hir_switch(subject_hir, hir_arms, hir_default, true);
                    self.hir_set_local_read(tmp, ty);
                }
                _ => {
                    self.hir_fail();
                    self.hir_none();
                }
            }
        } else {
            self.hir_switch(subject_hir, hir_arms, hir_default, hir_ok);
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
