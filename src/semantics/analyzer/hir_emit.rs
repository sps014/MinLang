//! Interleaved HIR emission (Step B of the architecture migration).
//!
//! As the analyzer type-checks a function it *also* builds the typed, name-resolved
//! [`crate::hir`] for it — the single-source-of-truth approach: there is no second type
//! inference pass. During the migration this is wired in incrementally: each expression records its
//! [`HExpr`] into [`HirEmit::last`] (a transition side-channel that avoids churning the ~50
//! `analyze_expression` call sites at once) and each supported statement appends an [`HStmt`]. A
//! function is emitted only if *every* construct in it is already supported; anything not yet
//! handled flips [`HirEmit::ok`] to `false` and the function is skipped, so the analyzer's behavior
//! and the legacy backend are unaffected. Coverage grows slice by slice until the analyzer emits HIR
//! for the whole language, at which point the driver switches to the MIR backend.

use super::Analyzer;
use crate::hir::{
    BinOp, Binding, Callee, GlobalId, HArm, HExpr, HExprKind, HFunction, HGlobal, HImport, HLocal,
    HParam, HPattern, HPlace, HStmt, LocalId, UnOp,
};
use crate::syntax::nodes::{FunctionNode, Type};
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::types::{DefId, DefKind, PrimTy, TyKind, TypeId};
use indexmap::IndexMap;

/// Per-analysis HIR-emission state, plus the accumulated [`HFunction`]s. Reset at the start of each
/// candidate function (see [`Analyzer::hir_begin_function`]).
#[derive(Default)]
pub(super) struct HirEmit {
    /// True while inside a function we are attempting to emit. When false, every helper is a no-op,
    /// so non-candidate functions (generic templates, methods, anything unsupported) cost nothing.
    collecting: bool,
    /// True while every construct seen in the current function has been representable in HIR. Once
    /// false, the function will not be emitted.
    ok: bool,
    /// The HIR of the most-recently-analyzed expression (`None` if it was not representable). A
    /// parent expression takes this immediately after analyzing each child.
    last: Option<HExpr>,
    /// Name -> (slot, type) for the current function's locals (parameters first, then `let`s).
    locals: IndexMap<String, (LocalId, TypeId)>,
    local_decls: Vec<HLocal>,
    params: Vec<HParam>,
    /// Stack of statement lists being built. The bottom is the function body; control-flow handlers
    /// push a frame for each nested block and pop it to attach to the enclosing statement.
    blocks: Vec<Vec<HStmt>>,
    def: Option<DefId>,
    name: String,
    /// The monomorphization type-args of the function currently being emitted (empty for a plain,
    /// non-generic function). Together with `def` this determines the emitted symbol, so a generic
    /// instance body and its call sites agree.
    instance: Vec<TypeId>,
    ret: Option<TypeId>,
    is_async: bool,
    /// Name -> (slot, type) for module-level variables, populated once after globals are analyzed
    /// (see [`Analyzer::hir_register_globals`]). Read by identifier/assignment lowering so a name
    /// that is not a local resolves to a [`Binding::Global`].
    globals: IndexMap<String, (GlobalId, TypeId)>,
    /// Captured global initializer expressions, keyed by variable name, attached to the matching
    /// [`HGlobal`] in [`Analyzer::hir_register_globals`]. Populated while top-level variables are
    /// analyzed (see [`Analyzer::hir_global_init_begin`]).
    pending_global_inits: IndexMap<String, HExpr>,
    /// All successfully emitted functions, surfaced via `SemanticInfo::hir`.
    pub functions: Vec<HFunction>,
    /// The module-global declarations, surfaced via `SemanticInfo::hir`.
    pub global_decls: Vec<HGlobal>,
}

/// Maps a surface binary operator token to the IR operator, or `None` for operators not yet lowered
/// by the interleaved emitter (short-circuiting `&&`/`||` and `??`, which desugar to control flow).
fn token_to_binop(kind: TokenKind) -> Option<BinOp> {
    Some(match kind {
        TokenKind::PlusToken => BinOp::Add,
        TokenKind::MinusToken => BinOp::Sub,
        TokenKind::StarToken => BinOp::Mul,
        TokenKind::SlashToken => BinOp::Div,
        TokenKind::ModulusToken => BinOp::Rem,
        TokenKind::EqualEqualToken => BinOp::Eq,
        TokenKind::NotEqualToken => BinOp::Ne,
        TokenKind::GreaterThanToken => BinOp::Gt,
        TokenKind::GreaterThanEqualToken => BinOp::Ge,
        TokenKind::SmallerThanToken => BinOp::Lt,
        TokenKind::SmallerThanEqualToken => BinOp::Le,
        TokenKind::BitWiseAmpersandToken => BinOp::BitAnd,
        TokenKind::BitWisePipeToken => BinOp::BitOr,
        TokenKind::BitWiseXorToken => BinOp::BitXor,
        TokenKind::ShiftLeftToken => BinOp::Shl,
        TokenKind::ShiftRightToken => BinOp::Shr,
        // Short-circuiting connectives: the MIR lowerer materializes these as branches
        // (`lower_short_circuit`), so they never reach the backend as a plain binary op.
        TokenKind::AmpersandAmpersandToken => BinOp::And,
        TokenKind::PipePipeToken => BinOp::Or,
        _ => return None,
    })
}

impl<'a> Analyzer<'a> {
    /// Starts HIR collection for `function`, returning whether it is a candidate. Slice 1 emits only
    /// plain non-generic, non-static free functions (no `this` receiver) that are registered as a
    /// `DefId`; everything else is skipped (collection stays off).
    pub(super) fn hir_begin_function(&mut self, function: &FunctionNode<'a>) {
        // `extern` functions are declarations with no body: host-interop imports are emitted as
        // `(import ...)` (see `hir_build_imports`) and `@intrinsic` ones lower straight to their
        // runtime helper (e.g. `String.alloc` → `$string_alloc`). Emitting an (empty) HIR body for
        // them would define a second `$string_alloc`, colliding with the runtime function.
        if function.is_extern {
            self.hir.collecting = false;
            return;
        }
        let is_generic = function
            .generic_parameters
            .as_ref()
            .is_some_and(|p| !p.is_empty());
        // Methods are registered (and looked up here) under their mangled `{Type}_{method}` name;
        // `this` is simply parameter 0. Static methods have no receiver. Both are emittable.
        let def = self
            .type_ctx
            .defs
            .lookup(DefKind::Function, &function.name.text);

        // A generic template is emitted once per monomorphization: the initial (unbound) pass is
        // skipped, and each concrete instantiation is analyzed again under `current_generic_bindings`
        // (see `analyze_pending_instantiations`). Anything with no registered def is skipped.
        let under_mono = !self.current_generic_bindings.is_empty();
        if def.is_none() || (is_generic && !under_mono) {
            self.hir.collecting = false;
            return;
        }
        // The instance type-args disambiguate the emitted symbol, but *only* for defs whose name is
        // shared across instantiations — i.e. generic free functions/methods, registered under their
        // base name. A method on a generic struct (`Box<int>.get`) is a non-generic method whose
        // specialization is already baked into its mangled `{Type_args}_{method}` def name, so it
        // takes an empty instance (its call sites resolve to that same mangled name with no suffix).
        let instance: Vec<TypeId> = if is_generic && under_mono {
            let concrete: Vec<String> = self
                .current_generic_bindings
                .iter()
                .map(|(_, c)| c.clone())
                .collect();
            concrete.iter().map(|c| self.type_ctx.lower_str(c)).collect()
        } else {
            Vec::new()
        };

        self.hir.collecting = true;
        self.hir.ok = true;
        self.hir.last = None;
        self.hir.locals.clear();
        self.hir.local_decls.clear();
        self.hir.params.clear();
        self.hir.blocks.clear();
        self.hir.blocks.push(Vec::new());
        self.hir.def = def;
        self.hir.instance = instance;
        self.hir.name = function.name.text.clone();
        self.hir.is_async = function.is_async;
        self.hir.ret = Some(
            function
                .return_type
                .as_ref()
                .map(|t| self.type_ctx.lower(t))
                .unwrap_or_else(|| self.type_ctx.interner.void()),
        );

        for param in function.parameters.iter() {
            let ty = self.type_ctx.lower(&param.type_);
            let local = LocalId(self.hir.locals.len() as u32);
            self.hir
                .locals
                .insert(param.name.text.clone(), (local, ty));
            self.hir.params.push(HParam {
                local,
                name: param.name.text.clone(),
                ty,
            });
        }
    }

    /// Finishes the current function: if it was a fully-supported candidate, builds and records its
    /// [`HFunction`]. Always turns collection back off.
    pub(super) fn hir_finish_function(&mut self) {
        // A well-formed function leaves exactly the body frame on the stack; a mismatch means an
        // unbalanced push/pop, so refuse to emit rather than emit a truncated body.
        if self.hir.collecting && self.hir.ok && self.hir.blocks.len() == 1 {
            if let (Some(def), Some(ret)) = (self.hir.def, self.hir.ret) {
                let body = self.hir.blocks.pop().unwrap_or_default();
                self.hir.functions.push(HFunction {
                    def,
                    name: std::mem::take(&mut self.hir.name),
                    instance: std::mem::take(&mut self.hir.instance),
                    params: std::mem::take(&mut self.hir.params),
                    ret,
                    locals: std::mem::take(&mut self.hir.local_decls),
                    body,
                    is_async: self.hir.is_async,
                });
            }
        }
        self.hir.blocks.clear();
        self.hir.collecting = false;
    }

    /// Takes the HIR recorded for the most-recently-analyzed expression.
    pub(super) fn hir_take(&mut self) -> Option<HExpr> {
        self.hir.last.take()
    }

    /// Marks the most-recent expression as not representable in HIR (clears `last`).
    pub(super) fn hir_none(&mut self) {
        self.hir.last = None;
    }

    /// Flags the current function as not emittable (an unsupported construct was reached).
    pub(super) fn hir_fail(&mut self) {
        if self.hir.collecting {
            self.hir.ok = false;
        }
    }

    fn active(&self) -> bool {
        self.hir.collecting && self.hir.ok
    }

    /// Appends a statement to the current (innermost) block, if collection is active.
    fn push_stmt(&mut self, stmt: HStmt) {
        if self.active() {
            if let Some(block) = self.hir.blocks.last_mut() {
                block.push(stmt);
            }
        }
    }

    /// Opens a nested statement block (e.g. a loop body). Paired with [`Self::hir_close_block`].
    /// Gated on `collecting` (not `ok`) so push/pop stay balanced even after the function is doomed.
    pub(super) fn hir_open_block(&mut self) {
        if self.hir.collecting {
            self.hir.blocks.push(Vec::new());
        }
    }

    /// Closes the innermost block and returns its statements.
    pub(super) fn hir_close_block(&mut self) -> Vec<HStmt> {
        if self.hir.collecting {
            self.hir.blocks.pop().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Allocates a fresh local slot without emitting a `let` (for loop-bound variables like a
    /// `foreach` element). Returns the slot, or `None` if collection is inactive.
    pub(super) fn hir_alloc_local(&mut self, name: &str, ty: &Type) -> Option<LocalId> {
        self.alloc_local(name, ty)
    }

    fn alloc_local(&mut self, name: &str, ty: &Type) -> Option<LocalId> {
        if !self.active() {
            return None;
        }
        let ty = self.type_ctx.lower(ty);
        let local = LocalId(self.hir.locals.len() as u32);
        self.hir.locals.insert(name.to_string(), (local, ty));
        self.hir.local_decls.push(HLocal {
            id: local,
            name: name.to_string(),
            ty,
        });
        Some(local)
    }

    /// Records the HIR for a literal expression.
    pub(super) fn hir_set_literal(&mut self, lit: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let kind = match lit {
            Type::Integer(t) | Type::Long(t) | Type::UInt(t) | Type::ULong(t) | Type::Byte(t) => {
                t.text.parse::<i64>().ok().map(HExprKind::IntLit)
            }
            Type::Float(t) | Type::Double(t) => t.text.parse::<f64>().ok().map(HExprKind::FloatLit),
            Type::Boolean(t) => Some(HExprKind::BoolLit(t.text == "true")),
            // The parser normalizes a char literal's token text to its decimal code point (e.g.
            // `'A'` → "65"), so recover the `char` from that integer rather than the raw glyph.
            Type::Char(t) => t
                .text
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(HExprKind::CharLit),
            Type::String(t) => Some(HExprKind::StringLit(string_lit_value(&t.text))),
            // The parser models the bare `null` literal as `Nullable(Void)` until its type is known.
            Type::Nullable(inner) if matches!(**inner, Type::Void) => Some(HExprKind::Null),
            _ => None,
        };
        let mut ty = self.type_ctx.lower(lit);
        // An `int`-typed literal whose value doesn't fit in `i32` is really a `long`: promote its HIR
        // type so the backend emits `i64.const` instead of an out-of-range `i32.const`. (The parser
        // types decimal integer literals as `int` regardless of magnitude.)
        if let Some(HExprKind::IntLit(v)) = &kind {
            if matches!(lit, Type::Integer(_)) && (*v > i32::MAX as i64 || *v < i32::MIN as i64) {
                ty = self.type_ctx.interner.long();
            }
        }
        self.hir.last = kind.map(|k| HExpr::new(ty, k));
    }

    /// Records the HIR for an identifier read: a local-variable reference if the name resolves to a
    /// slot, otherwise `None` (globals and function values are later slices).
    pub(super) fn hir_set_var(&mut self, name: &str) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        if let Some(&(local, ty)) = self.hir.locals.get(name) {
            self.hir.last = Some(HExpr::new(ty, HExprKind::Var(Binding::Local(local))));
        } else if let Some(&(global, ty)) = self.hir.globals.get(name) {
            self.hir.last = Some(HExpr::new(ty, HExprKind::Var(Binding::Global(global))));
        } else {
            self.hir.last = None;
        }
    }

    /// Records the HIR for a binary expression from its already-collected operands.
    pub(super) fn hir_set_binary(
        &mut self,
        lhs: Option<HExpr>,
        opr: &SyntaxToken,
        rhs: Option<HExpr>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match (token_to_binop(opr.kind), lhs, rhs) {
            (Some(op), Some(lhs), Some(rhs)) => {
                let ty = self.type_ctx.lower(result_ty);
                self.hir.last = Some(HExpr::new(
                    ty,
                    HExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                ));
            }
            _ => self.hir.last = None,
        }
    }

    /// Records the HIR for a unary expression. Unary `+` is the identity (passes the operand
    /// through); `-` and `!` map to [`UnOp::Neg`]/[`UnOp::Not`].
    pub(super) fn hir_set_unary(&mut self, opr: &SyntaxToken, operand: Option<HExpr>, result_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let op = match opr.kind {
            TokenKind::PlusToken => {
                self.hir.last = operand;
                return;
            }
            TokenKind::MinusToken => UnOp::Neg,
            TokenKind::BangToken => UnOp::Not,
            _ => {
                self.hir.last = None;
                return;
            }
        };
        self.hir.last = operand.map(|operand| {
            let ty = self.type_ctx.lower(result_ty);
            HExpr::new(
                ty,
                HExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
            )
        });
    }

    /// Records the HIR for a `cond ? then : else_` from its already-collected parts.
    pub(super) fn hir_set_ternary(
        &mut self,
        cond: Option<HExpr>,
        then_e: Option<HExpr>,
        else_e: Option<HExpr>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match (cond, then_e, else_e) {
            (Some(cond), Some(then_expr), Some(else_expr)) => {
                let ty = self.type_ctx.lower(result_ty);
                self.hir.last = Some(HExpr::new(
                    ty,
                    HExprKind::Ternary {
                        cond: Box::new(cond),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    },
                ));
            }
            _ => self.hir.last = None,
        }
    }

    /// Records the HIR for null-coalescing `lhs ?? rhs`.
    pub(super) fn hir_set_coalesce(
        &mut self,
        lhs: Option<HExpr>,
        rhs: Option<HExpr>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match (lhs, rhs) {
            (Some(lhs), Some(rhs)) => {
                let ty = self.type_ctx.lower(result_ty);
                self.hir.last = Some(HExpr::new(
                    ty,
                    HExprKind::Coalesce {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                ));
            }
            _ => self.hir.last = None,
        }
    }

    /// Records the HIR for `array[index]` (read position).
    pub(super) fn hir_set_index(
        &mut self,
        array: Option<HExpr>,
        index: Option<HExpr>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match (array, index) {
            (Some(array), Some(index)) => {
                let ty = self.type_ctx.lower(result_ty);
                self.hir.last = Some(HExpr::new(
                    ty,
                    HExprKind::Index {
                        array: Box::new(array),
                        index: Box::new(index),
                    },
                ));
            }
            _ => self.hir.last = None,
        }
    }

    /// Records the HIR for a cast `expr as T`; `target_ty` is the cast's result type.
    pub(super) fn hir_set_cast(&mut self, inner: Option<HExpr>, target_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        self.hir.last = inner.map(|inner| {
            let ty = self.type_ctx.lower(target_ty);
            HExpr::new(ty, HExprKind::Cast(Box::new(inner)))
        });
    }

    /// Records the HIR for a non-empty array literal. `result_ty` is the array type (`T[]`); the
    /// element type is taken from it. Fails if any element was not representable.
    pub(super) fn hir_set_array_lit(&mut self, elems: Vec<Option<HExpr>>, result_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let elem_ty = match result_ty {
            Type::Array(inner) => self.type_ctx.lower(inner),
            _ => {
                self.hir.last = None;
                return;
            }
        };
        let mut collected = Vec::with_capacity(elems.len());
        for e in elems {
            match e {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ty = self.type_ctx.lower(result_ty);
        self.hir.last = Some(HExpr::new(
            ty,
            HExprKind::ArrayLit {
                elem_ty,
                elems: collected,
            },
        ));
    }

    /// Records the HIR for a direct free-function call `name(args)`. Resolves `name` to its function
    /// `DefId`; if it is not a registered (non-generic, non-overloaded) function or any argument is
    /// not representable, the call is dropped from coverage (the function falls back to the legacy
    /// path).
    pub(super) fn hir_set_call(&mut self, name: &str, args: Vec<Option<HExpr>>, ret: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, name) else {
            self.hir.last = None;
            return;
        };
        let mut collected = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ret_ty = self.type_ctx.lower(ret);
        let callee = Callee {
            def,
            instance: vec![],
            ret: ret_ty,
        };
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::Call {
                callee,
                args: collected,
            },
        ));
    }

    /// Records a first-class function value: a bare function name used as a value (e.g. `let f = foo;`
    /// or passing `foo` to a `fun(...)` parameter) becomes a `Binding::Func` carrying the resolved def
    /// and signature, typed as the function type so the backend can materialize its table index. Drops
    /// coverage if the name is not a registered function def.
    pub(super) fn hir_set_func_value(&mut self, name: &str, func_ty: &Type, ret: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, name) else {
            self.hir.last = None;
            return;
        };
        let tid = self.type_ctx.lower(func_ty);
        let ret_ty = self.type_ctx.lower(ret);
        self.hir.last = Some(HExpr::new(
            tid,
            HExprKind::Var(Binding::Func(Callee { def, instance: vec![], ret: ret_ty })),
        ));
    }

    /// Records an indirect call `f(args)` where `f` is a function-typed local: the target reads the
    /// local (whose value is a function-table index) and the call dispatches through it. Drops coverage
    /// if the name is not a known local or any argument is not representable.
    pub(super) fn hir_set_indirect_call(&mut self, name: &str, args: Vec<Option<HExpr>>, ret: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(&(local, ty)) = self.hir.locals.get(name) else {
            self.hir.last = None;
            return;
        };
        let mut collected = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ret_ty = self.type_ctx.lower(ret);
        let target = HExpr::new(ty, HExprKind::Var(Binding::Local(local)));
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::IndirectCall { target: Box::new(target), args: collected },
        ));
    }

    /// Records the HIR for a resolved call to a generic free function. `base_name` is the template's
    /// (unmangled) name — the `DefId` shared by every instance — and `instance` is the concrete
    /// type-args (in binding order) that select the monomorphization. The backend combines
    /// `(def, instance)` into the same symbol the instance body emits. Drops out of coverage if the
    /// base name is unregistered or any argument is not representable.
    pub(super) fn hir_set_generic_call(
        &mut self,
        base_name: &str,
        instance: Vec<TypeId>,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, base_name) else {
            self.hir.last = None;
            return;
        };
        let mut collected = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ret_ty = self.type_ctx.lower(ret);
        let callee = Callee {
            def,
            instance,
            ret: ret_ty,
        };
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::Call {
                callee,
                args: collected,
            },
        ));
    }

    /// Records the HIR for an enum-member reference (`Enum.Member`) resolved to its integer value.
    pub(super) fn hir_set_enum_value(&mut self, value: i64, enum_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let ty = self.type_ctx.lower(enum_ty);
        self.hir.last = Some(HExpr::new(ty, HExprKind::EnumValue(value)));
    }

    /// Records the HIR for a struct field read `obj.field`; `field` is the resolved field index
    /// (offset order). Fails over to the legacy path if the receiver was not representable.
    pub(super) fn hir_set_field(&mut self, obj: Option<HExpr>, field: usize, field_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        self.hir.last = obj.map(|obj| {
            let ty = self.type_ctx.lower(field_ty);
            HExpr::new(
                ty,
                HExprKind::Field {
                    obj: Box::new(obj),
                    field,
                },
            )
        });
    }

    /// Records the HIR for a constructor call `Struct(args)`. `name` is the source (base) struct name
    /// — the registered `DefId` for both plain and generic structs — and `result_ty` supplies the
    /// per-instance layout key. `ctor`, when `Some`, is the resolved user `constructor(){}` def (its
    /// `args` are the constructor's arguments); when `None`, `args` initialize fields positionally.
    /// Unresolved names or a non-representable argument drop the call out of coverage.
    pub(super) fn hir_set_new(
        &mut self,
        name: &str,
        ctor: Option<DefId>,
        args: Vec<Option<HExpr>>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.defs.lookup(DefKind::Struct, name) else {
            self.hir.last = None;
            return;
        };
        let mut collected = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ty = self.type_ctx.lower(result_ty);
        self.hir.last = Some(HExpr::new(
            ty,
            HExprKind::New {
                def,
                instance: vec![],
                ctor,
                args: collected,
            },
        ));
    }

    /// Records a resolved instance method call `receiver.method(args)`. `mangled` is the registered
    /// `{Type}_{method}` name; if it does not resolve to a `DefId`, or the receiver/any argument is
    /// not representable, the call drops out of coverage.
    pub(super) fn hir_set_method_call(
        &mut self,
        receiver: Option<HExpr>,
        mangled: &str,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let (Some(def), Some(receiver)) = (
            self.type_ctx.defs.lookup(DefKind::Function, mangled),
            receiver,
        ) else {
            self.hir.last = None;
            return;
        };
        let mut collected = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ret_ty = self.type_ctx.lower(ret);
        let callee = Callee {
            def,
            instance: vec![],
            ret: ret_ty,
        };
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::MethodCall {
                receiver: Box::new(receiver),
                callee,
                args: collected,
            },
        ));
    }

    /// Records a discriminated-union construction `Enum.Variant(args)`. `def` is the union's `DefId`
    /// and `variant` its discriminant; any non-representable argument drops it out of coverage.
    pub(super) fn hir_set_union_new(
        &mut self,
        def: DefId,
        variant: usize,
        args: Vec<Option<HExpr>>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let mut collected = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Some(e) => collected.push(e),
                None => {
                    self.hir.last = None;
                    return;
                }
            }
        }
        let ty = self.type_ctx.lower(result_ty);
        self.hir.last = Some(HExpr::new(
            ty,
            HExprKind::UnionNew {
                def,
                variant,
                args: collected,
            },
        ));
    }

    /// Turns on HIR collection so a top-level variable's initializer expression is captured while it
    /// is analyzed. There is no enclosing function, so there are no locals/blocks — only the top
    /// expression's HIR is wanted. Paired with [`Self::hir_global_init_finish`].
    pub(super) fn hir_global_init_begin(&mut self) {
        self.hir.collecting = true;
        self.hir.ok = true;
        self.hir.last = None;
    }

    /// Stores the captured initializer for global `name` (if it was fully representable) and turns
    /// collection back off.
    pub(super) fn hir_global_init_finish(&mut self, name: &str) {
        if self.hir.collecting && self.hir.ok {
            if let Some(init) = self.hir.last.take() {
                self.hir.pending_global_inits.insert(name.to_string(), init);
            }
        }
        self.hir.collecting = false;
        self.hir.last = None;
    }

    /// Registers one top-level variable's HIR slot as it is analyzed (in declaration order), so a
    /// *later* global's initializer can resolve an *earlier* global to a [`Binding::Global`]. The
    /// slot `id` must equal the variable's index in [`Analyzer::globals`]. The initializer captured
    /// by [`Self::hir_global_init_finish`] (if representable) is attached to the surfaced [`HGlobal`].
    pub(super) fn hir_register_global(&mut self, name: &str, type_str: &str, is_const: bool) {
        let ty = self.type_ctx.lower_str(type_str);
        let id = GlobalId(self.hir.globals.len() as u32);
        self.hir.globals.insert(name.to_string(), (id, ty));
        let init = self.hir.pending_global_inits.shift_remove(name);
        self.hir.global_decls.push(HGlobal {
            id,
            name: name.to_string(),
            ty,
            is_const,
            init,
        });
    }

    /// Builds the [`crate::hir::LayoutTable`] from the analyzed struct and union tables: each struct's
    /// `DefId` maps to its field offsets/sizes, and each union's `DefId` to its per-variant
    /// discriminant + payload offsets, so the backend can lower `obj.field` reads and `new`/variant
    /// construction to concrete loads/stores.
    pub(super) fn hir_build_layouts(&mut self) -> crate::hir::LayoutTable {
        use crate::hir::{FieldLayout, LayoutTable, TypeLayout, UnionLayout, UnionVariant};
        // Snapshot field types in declaration order first, so `type_ctx` can be re-borrowed mutably
        // for lowering without aliasing the struct/union-table borrows.
        // Discriminated unions are also registered in the struct table (for tagging/release), but they
        // get a variant-aware layout + `to_string` from the union table below — so exclude them here to
        // avoid a duplicate (empty) struct layout and a duplicate `$<Union>_to_string`.
        let struct_snapshot: Vec<(String, Vec<(String, Type)>)> = self
            .struct_table
            .structs
            .iter()
            .filter(|(name, _)| !self.union_table.contains_key(name.as_str()))
            .map(|(name, info)| {
                let fields =
                    info.fields.iter().map(|(fname, f)| (fname.clone(), f.type_.clone())).collect();
                (name.clone(), fields)
            })
            .collect();
        // (union name, block size, [(variant name, discriminant, [(field name, offset, field type)])]).
        type VariantSnap = (String, i32, Vec<(String, u32, Type)>);
        let union_snapshot: Vec<(String, u32, Vec<VariantSnap>)> = self
            .union_table
            .iter()
            .map(|(name, info)| {
                let variants = info
                    .variants
                    .iter()
                    .map(|v| {
                        let fields = v
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.offset as u32, f.type_.clone()))
                            .collect();
                        (v.name.clone(), v.discriminant, fields)
                    })
                    .collect();
                (name.clone(), info.size as u32, variants)
            })
            .collect();

        let mut layouts = LayoutTable::default();
        for (name, fields) in struct_snapshot {
            // Key by the struct's interned type id (`lower_str` canonicalizes both plain names and
            // mangled generic instances like `Box_int` to `struct_ty(def, args)`), so each
            // monomorphization gets its own layout.
            let ty = self.type_ctx.lower_str(&name);
            let defs: Vec<(String, TypeId)> =
                fields.iter().map(|(fname, t)| (fname.clone(), self.type_ctx.lower(t))).collect();
            layouts.insert(ty, TypeLayout::from_fields(&self.type_ctx.interner, name, defs));
        }
        for (name, size, variants) in union_snapshot {
            let ty = self.type_ctx.lower_str(&name);
            let mut vs = Vec::with_capacity(variants.len());
            for (vname, discriminant, fields) in variants {
                let flds = fields
                    .into_iter()
                    .map(|(fname, offset, t)| FieldLayout {
                        offset,
                        ty: self.type_ctx.lower(&t),
                        name: fname,
                    })
                    .collect();
                vs.push(UnionVariant { name: vname, discriminant, fields: flds });
            }
            layouts.insert_union(ty, UnionLayout { name, variants: vs, size });
        }
        layouts
    }

    /// Collects the module's host/interop imports: every non-intrinsic `extern fun` (top-level or a
    /// class/`extend` static member) becomes an [`HImport`] the backend emits as `(import ...)`.
    /// Overloaded externs share one imported field, so entries are de-duplicated by name.
    pub(super) fn hir_build_imports(
        &mut self,
        node: &crate::syntax::nodes::ProgramNode,
    ) -> Vec<HImport> {
        let mut imports: Vec<HImport> = Vec::new();
        let class_methods = node.structs.iter().flat_map(|s| s.methods.iter());
        let extend_methods = node.extends.iter().flat_map(|e| e.methods.iter());
        for func in node.functions.iter().chain(class_methods).chain(extend_methods) {
            if !func.is_extern || crate::intrinsics::has_intrinsic_attr(&func.attributes) {
                continue;
            }
            if imports.iter().any(|i| i.name == func.name.text) {
                continue;
            }
            // Match the def the call site resolves to, so the emitter's symbol table maps the call
            // onto this import's `$name`. Unregistered externs (should not happen) are skipped.
            let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &func.name.text) else {
                continue;
            };
            let (module, field) = extern_import_target(func);
            let params = func
                .parameters
                .iter()
                .map(|p| self.type_ctx.lower(&p.type_))
                .collect();
            let ret = match func.return_type.as_ref() {
                Some(t) if *t != Type::Void => Some(self.type_ctx.lower(t)),
                _ => None,
            };
            imports.push(HImport { def, name: func.name.text.clone(), module, field, params, ret });
        }
        imports
    }

    /// Collects every `@intrinsic("key")` extern as `(callee DefId, key)`. Unlike host imports these
    /// have no `(import ...)` and no emitted body: their call sites resolve directly to the runtime
    /// helper `$<key>` (`string_alloc`, `char_at`, …) or, for `sleep`, are recognized as an async
    /// intrinsic. Methods are looked up under their mangled `{Type}_{method}` def name (the name the
    /// call site resolves to), matching how they were registered.
    pub(super) fn hir_build_intrinsics(
        &mut self,
        node: &crate::syntax::nodes::ProgramNode,
    ) -> Vec<(crate::types::DefId, String)> {
        use crate::syntax::nodes::types::method_fn;
        let mut out: Vec<(crate::types::DefId, String)> = Vec::new();
        for func in node.functions.iter() {
            if let Some(key) = crate::intrinsics::intrinsic_key(&func.attributes) {
                if let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &func.name.text) {
                    out.push((def, key));
                }
            }
        }
        for s in node.structs.iter() {
            for m in s.methods.iter() {
                if let Some(key) = crate::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&s.name.text, &m.name.text);
                    if let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        for e in node.extends.iter() {
            for m in e.methods.iter() {
                if let Some(key) = crate::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&e.target.text, &m.name.text);
                    if let Some(def) = self.type_ctx.defs.lookup(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        out
    }

    /// Records a `print`/`println` builtin call as [`HExprKind::Print`] (void). Every scalar
    /// primitive is covered: `int`/`char`/`string` go straight to a host import, while the other
    /// numerics and `bool` route through the in-wasm `*_to_string` runtime before `$print_string`.
    /// Non-scalar (object/struct/array) arguments still need the object-protocol `to_string` and so
    /// drop the enclosing function out of HIR coverage until that runtime lands.
    pub(super) fn hir_set_print(&mut self, arg: Option<HExpr>, newline: bool) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(arg) = arg else {
            self.hir.ok = false;
            self.hir.last = None;
            return;
        };
        let base = self.type_ctx.interner.strip_nullable(arg.ty);
        // Scalars print directly; enums print as their `int` value; every reference type (struct,
        // union, array, `object`) renders through the backend's tag-dispatching `$print_object`.
        let printable = matches!(
            self.type_ctx.interner.kind(base),
            TyKind::Prim(_) | TyKind::Enum(_) | TyKind::Struct(..) | TyKind::Union(..) | TyKind::Array(_) | TyKind::Object
        );
        if !printable {
            self.hir.ok = false;
            self.hir.last = None;
            return;
        }
        let void = self.type_ctx.interner.void();
        self.hir.last = Some(HExpr::new(
            void,
            HExprKind::Print { arg: Box::new(arg), newline },
        ));
    }

    /// Records `recv.len()` (typed `int`): an array reads its stored length word (`ArrayLen`), while a
    /// string scans for its NUL terminator at runtime (`StrLen`), since the two have different layouts.
    pub(super) fn hir_set_array_len(&mut self, recv: Option<HExpr>) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match recv {
            Some(e) => {
                let int = self.type_ctx.interner.int();
                let is_string = matches!(
                    self.type_ctx.interner.kind(self.type_ctx.interner.strip_nullable(e.ty)),
                    TyKind::Prim(PrimTy::String)
                );
                let kind = if is_string {
                    HExprKind::StrLen(Box::new(e))
                } else {
                    HExprKind::ArrayLen(Box::new(e))
                };
                self.hir.last = Some(HExpr::new(int, kind));
            }
            None => self.hir.last = None,
        }
    }

    /// Records the object-protocol `x.hash_code()` (typed `int`): the backend dispatches on the
    /// receiver's static type. Drops out of coverage if the receiver is not representable.
    pub(super) fn hir_set_hash_code(&mut self, recv: Option<HExpr>) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match recv {
            Some(e) => {
                let int = self.type_ctx.interner.int();
                self.hir.last = Some(HExpr::new(int, HExprKind::HashCode(Box::new(e))));
            }
            None => self.hir.last = None,
        }
    }

    /// Records the object-protocol `x.to_string()` (typed `string`): the backend dispatches on the
    /// receiver's static type. Drops out of coverage if the receiver is not representable.
    pub(super) fn hir_set_to_string(&mut self, recv: Option<HExpr>) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match recv {
            Some(e) => {
                let string = self.type_ctx.interner.prim(PrimTy::String);
                self.hir.last = Some(HExpr::new(string, HExprKind::ToString(Box::new(e))));
            }
            None => self.hir.last = None,
        }
    }

    /// Records `Array.new<T>(len)` (typed `T[]`): a zero-initialized array allocation. Drops out of
    /// coverage if the length is not representable.
    pub(super) fn hir_set_array_new(&mut self, elem_ty: &Type, len: Option<HExpr>) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match len {
            Some(len) => {
                let elem = self.type_ctx.lower(elem_ty);
                let arr = self.type_ctx.interner.array(elem);
                self.hir.last = Some(HExpr::new(
                    arr,
                    HExprKind::ArrayNew {
                        elem_ty: elem,
                        len: Box::new(len),
                    },
                ));
            }
            None => self.hir.last = None,
        }
    }

    /// Records `recv.char_at(idx)` (typed `char`): a runtime `$char_at` read. Drops out of coverage
    /// if either the receiver or the index is not representable.
    pub(super) fn hir_set_char_at(&mut self, recv: Option<HExpr>, idx: Option<HExpr>) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match (recv, idx) {
            (Some(r), Some(i)) => {
                let char_ty = self.type_ctx.interner.prim(PrimTy::Char);
                self.hir.last = Some(HExpr::new(
                    char_ty,
                    HExprKind::CharAt(Box::new(r), Box::new(i)),
                ));
            }
            _ => self.hir.last = None,
        }
    }

    /// Records `await e` used as a value (carrying the awaited future's inner type).
    pub(super) fn hir_set_await(&mut self, inner: Option<HExpr>, inner_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        match inner {
            Some(e) => {
                let ty = self.type_ctx.lower(inner_ty);
                self.hir.last = Some(HExpr::new(ty, HExprKind::Await(Box::new(e))));
            }
            None => self.hir.last = None,
        }
    }

    /// Appends `await e;` at statement position.
    pub(super) fn hir_await_stmt(&mut self, value: Option<HExpr>) {
        if !self.active() {
            return;
        }
        match value {
            Some(v) => self.push_stmt(HStmt::Await(v)),
            None => self.hir.ok = false,
        }
    }

    /// Sets `last` to a read of an already-allocated local (used by the match-expression desugar to
    /// yield the result temporary as the match's value).
    pub(super) fn hir_set_local_read(&mut self, local: LocalId, ty: TypeId) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        self.hir.last = Some(HExpr::new(ty, HExprKind::Var(Binding::Local(local))));
    }

    /// Appends an assignment to an already-allocated local slot (used by the match-expression
    /// desugar's result temporary).
    pub(super) fn hir_assign_local_id(&mut self, local: LocalId, value: Option<HExpr>) {
        if !self.active() {
            return;
        }
        match value {
            Some(value) => self.push_stmt(HStmt::Assign {
                place: HPlace::Local(local),
                value,
            }),
            None => self.hir.ok = false,
        }
    }

    /// Appends a field assignment `obj.field = value;` (`field` is the resolved offset-order index).
    pub(super) fn hir_assign_field(
        &mut self,
        obj: Option<HExpr>,
        field: usize,
        value: Option<HExpr>,
    ) {
        if !self.active() {
            return;
        }
        match (obj, value) {
            (Some(obj), Some(value)) => self.push_stmt(HStmt::Assign {
                place: HPlace::Field {
                    obj: Box::new(obj),
                    field,
                },
                value,
            }),
            _ => self.hir.ok = false,
        }
    }

    /// Appends an indexed assignment `array[index] = value;`.
    pub(super) fn hir_assign_index(
        &mut self,
        array: Option<HExpr>,
        index: Option<HExpr>,
        value: Option<HExpr>,
    ) {
        if !self.active() {
            return;
        }
        match (array, index, value) {
            (Some(array), Some(index), Some(value)) => self.push_stmt(HStmt::Assign {
                place: HPlace::Index {
                    array: Box::new(array),
                    index: Box::new(index),
                },
                value,
            }),
            _ => self.hir.ok = false,
        }
    }

    /// Appends a `let` binding, allocating a fresh local slot. Fails the function if the initializer
    /// was not representable.
    pub(super) fn hir_declare_local(&mut self, name: &str, ty: &Type, value: Option<HExpr>) {
        if !self.active() {
            return;
        }
        let Some(value) = value else {
            self.hir.ok = false;
            return;
        };
        let ty = self.type_ctx.lower(ty);
        let local = LocalId(self.hir.locals.len() as u32);
        self.hir.locals.insert(name.to_string(), (local, ty));
        self.hir.local_decls.push(HLocal {
            id: local,
            name: name.to_string(),
            ty,
        });
        self.push_stmt(HStmt::Let { local, ty, value });
    }

    /// Appends an assignment to a local or module-global. Fails the function for an unresolved name
    /// or a non-representable value.
    pub(super) fn hir_assign_local(&mut self, name: &str, value: Option<HExpr>) {
        if !self.active() {
            return;
        }
        let Some(value) = value else {
            self.hir.ok = false;
            return;
        };
        if let Some(&(local, _)) = self.hir.locals.get(name) {
            self.push_stmt(HStmt::Assign {
                place: HPlace::Local(local),
                value,
            });
        } else if let Some(&(global, _)) = self.hir.globals.get(name) {
            self.push_stmt(HStmt::Assign {
                place: HPlace::Global(global),
                value,
            });
        } else {
            self.hir.ok = false;
        }
    }

    /// Appends `return value;`, failing the function if the value was not representable.
    pub(super) fn hir_return_value(&mut self, value: Option<HExpr>) {
        if !self.active() {
            return;
        }
        match value {
            Some(value) => self.push_stmt(HStmt::Return(Some(value))),
            None => self.hir.ok = false,
        }
    }

    /// Appends a bare `return;`.
    pub(super) fn hir_return_void(&mut self) {
        self.push_stmt(HStmt::Return(None));
    }

    /// Appends an expression statement, failing the function if it was not representable.
    pub(super) fn hir_expr_stmt(&mut self, value: Option<HExpr>) {
        if !self.active() {
            return;
        }
        match value {
            Some(value) => self.push_stmt(HStmt::Expr(value)),
            None => self.hir.ok = false,
        }
    }

    /// Appends a `while (cond) { body }`. Fails the function if the condition was not representable.
    pub(super) fn hir_while(&mut self, cond: Option<HExpr>, body: Vec<HStmt>) {
        if !self.active() {
            return;
        }
        match cond {
            Some(cond) => self.push_stmt(HStmt::While { cond, body }),
            None => self.hir.ok = false,
        }
    }

    /// Appends an `if`/`else if`/`else` chain, folding the `else if`s into nested `else` branches.
    /// `primary` is the leading condition+body; `elifs` the `else if`s in source order; `else_block`
    /// the trailing `else` (empty if absent). Fails the function if any condition was unrepresentable.
    pub(super) fn hir_if_chain(
        &mut self,
        primary: (Option<HExpr>, Vec<HStmt>),
        elifs: Vec<(Option<HExpr>, Vec<HStmt>)>,
        else_block: Vec<HStmt>,
    ) {
        if !self.active() {
            return;
        }
        let mut else_branch = else_block;
        for (cond, body) in elifs.into_iter().rev() {
            let Some(cond) = cond else {
                self.hir.ok = false;
                return;
            };
            else_branch = vec![HStmt::If {
                cond,
                then_branch: body,
                else_branch,
            }];
        }
        let (Some(cond), then_branch) = (primary.0, primary.1) else {
            self.hir.ok = false;
            return;
        };
        self.push_stmt(HStmt::If {
            cond,
            then_branch,
            else_branch,
        });
    }

    /// Appends a desugared `for (init; cond; step) { body }`. `init`/`step` must each be exactly one
    /// statement (the surface form guarantees this) and `cond` must be present.
    pub(super) fn hir_for(
        &mut self,
        mut init: Vec<HStmt>,
        cond: Option<HExpr>,
        mut step: Vec<HStmt>,
        body: Vec<HStmt>,
    ) {
        if !self.active() {
            return;
        }
        match (init.len(), step.len(), cond) {
            (1, 1, Some(cond)) => self.push_stmt(HStmt::For {
                init: Box::new(init.remove(0)),
                cond,
                step: Box::new(step.remove(0)),
                body,
            }),
            _ => self.hir.ok = false,
        }
    }

    /// Appends `foreach (elem in iterable) { body }`. `elem` is the slot allocated (before the body
    /// was analyzed, so the body can resolve the element) via [`Self::hir_alloc_local`].
    pub(super) fn hir_foreach(
        &mut self,
        elem: Option<LocalId>,
        iterable: Option<HExpr>,
        body: Vec<HStmt>,
    ) {
        if !self.active() {
            return;
        }
        match (elem, iterable) {
            (Some(elem), Some(iterable)) => {
                self.push_stmt(HStmt::Foreach { elem, iterable, body })
            }
            _ => self.hir.ok = false,
        }
    }

    /// Appends a `break`/`continue` (with optional loop label).
    pub(super) fn hir_break(&mut self, label: Option<String>) {
        self.push_stmt(HStmt::Break(label));
    }

    pub(super) fn hir_continue(&mut self, label: Option<String>) {
        self.push_stmt(HStmt::Continue(label));
    }

    /// Appends a `switch`/statement-`match` lowered to [`HStmt::Switch`]. `arms` are the already-built
    /// pattern/body pairs and `default` the fallthrough block. `ok` is the caller's verdict on
    /// whether every arm was representable (e.g. no multi-label case, scrutinee present); a `false`
    /// verdict, a missing scrutinee, or inactive collection fails the function.
    pub(super) fn hir_switch(
        &mut self,
        scrutinee: Option<HExpr>,
        arms: Vec<HArm>,
        default: Vec<HStmt>,
        ok: bool,
    ) {
        if !self.active() {
            return;
        }
        match scrutinee {
            Some(scrutinee) if ok => self.push_stmt(HStmt::Switch {
                scrutinee,
                arms,
                default,
            }),
            _ => self.hir.ok = false,
        }
    }

    /// Builds a `Const` switch arm from a label expression (the case value).
    pub(super) fn hir_const_arm(&self, label: Option<HExpr>, body: Vec<HStmt>) -> Option<HArm> {
        label.map(|label| HArm {
            pattern: HPattern::Const(label),
            body,
        })
    }

    /// Builds a `Variant` match arm (`Enum.Variant(bindings...) => body`). `bindings` are the local
    /// slots already allocated for the payload (in field order).
    pub(super) fn hir_variant_arm(
        &self,
        def: DefId,
        variant: usize,
        bindings: Vec<LocalId>,
        body: Vec<HStmt>,
    ) -> HArm {
        HArm {
            pattern: HPattern::Variant {
                def,
                variant,
                bindings,
            },
            body,
        }
    }

}

/// The `(module, field)` an `extern fun` imports from: `("env", <name>)` by default, overridable via
/// `@js("module", "field")` (mirrors the ABI sidecar emitter so calls and metadata agree).
fn extern_import_target(func: &FunctionNode) -> (String, String) {
    let mut module = "env".to_string();
    let mut field = func.name.text.clone();
    if let Some(js) = func.attributes.iter().find(|a| a.name.text == "js") {
        if let Some(arg) = js.args.first() {
            module = arg.text.trim_matches('"').to_string();
        }
        if let Some(arg) = js.args.get(1) {
            field = arg.text.trim_matches('"').to_string();
        }
    }
    (module, field)
}

/// Expands the backslash escapes a string/char literal body may contain (`\n`, `\t`, `\r`, `\0`,
/// `\\`, `\"`, `\'`). Unknown escapes keep the escaped character verbatim, matching the lexer's
/// permissive stance.
fn unescape_lit_body(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The runtime content of a string literal: the raw token text still carries its surrounding double
/// quotes (it is the source slice), so strip them and expand escapes. Idempotent on already-unquoted
/// input.
fn string_lit_value(text: &str) -> String {
    let body = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(text);
    unescape_lit_body(body)
}

