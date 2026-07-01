//! The Mid-level IR (MIR): a control-flow graph of basic blocks with explicit, low-level
//! operations.
//!
//! Where HIR keeps structured control flow, MIR desugars everything (if/while/for/foreach/switch/
//! match/ternary/`&&`/`||`/`??`/async) into blocks joined by [`Terminator`]s. Reference-counting
//! (`Retain`/`Release`) and allocation are explicit [`Statement`]s, which lets the optimization
//! passes reason about them with ordinary dataflow. The backend  reconstructs
//! structured WASM control flow from this CFG via a relooper.

pub mod async_emit;
pub mod build;
pub mod emit;
pub mod lower;
pub mod passes;
pub mod print;
pub mod relooper;

pub use crate::hir::{BinOp, UnOp};
use crate::types::{DefId, TypeId};

/// A basic block within a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

/// An SSA-style local. Locals are the only values; every intermediate result is materialized into a
/// local, so operands are either locals or constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Local(pub u32);

/// A module-level global slot (mirrors `hir::GlobalId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Global(pub u32);

/// A whole program in MIR form.
#[derive(Debug, Default)]
pub struct Mir {
    pub functions: Vec<MirFunction>,
    /// Module-level variable slots, so the backend can declare a WASM global per slot.
    pub globals: Vec<MirGlobal>,
    /// Field/offset layout of every nominal type, carried from HIR for the backend to lower
    /// field/index access.
    pub layouts: crate::hir::LayoutTable,
    /// Host/extern imports, carried verbatim from HIR for the backend to emit `(import ...)`.
    pub imports: Vec<crate::hir::HImport>,
    /// `@intrinsic` externs: `(callee DefId, intrinsic key)`. Carried from HIR so the backend's
    /// symbol table resolves intrinsic call targets (to the runtime helper `$<key>`, or the async
    /// scheduler for `sleep`) instead of the `$def{N}` fallback.
    pub intrinsics: Vec<(DefId, String)>,
}

/// A module-level variable slot (declared as one mutable WASM global `$g{id}`).
#[derive(Debug)]
pub struct MirGlobal {
    pub id: Global,
    pub ty: TypeId,
}

#[derive(Debug)]
pub struct MirFunction {
    /// The nominal def this function (or generic instance) belongs to. The emitted symbol is derived
    /// from `(def, instance)` so call sites and headers agree and generic instances stay distinct.
    pub def: DefId,
    /// Concrete type args when this is a monomorphized instance body; empty otherwise.
    pub instance: Vec<TypeId>,
    pub name: String,
    pub params: Vec<Local>,
    pub ret: TypeId,
    /// Typed declaration for every local (params included), indexed by `Local.0`.
    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
    pub is_async: bool,
    /// When `is_async`, the full typed HIR function preserved for the coroutine transform.
    pub hir_fn: Option<crate::hir::HFunction>,
}

impl MirFunction {
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.0 as usize]
    }

    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        &mut self.blocks[id.0 as usize]
    }

    pub fn local_ty(&self, local: Local) -> TypeId {
        self.locals[local.0 as usize].ty
    }
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub ty: TypeId,
    /// Optional source name (params/user `let`s); synthetic temporaries have `None`.
    pub name: Option<String>,
}

#[derive(Debug, Default)]
pub struct BasicBlock {
    pub stmts: Vec<Statement>,
    pub terminator: Terminator,
}

/// A straight-line operation with no control-flow effect.
#[derive(Debug, Clone)]
pub enum Statement {
    /// `place = rvalue`.
    Assign(Place, Rvalue),
    /// Increment the refcount of a reference operand.
    Retain(Operand),
    /// Decrement the refcount of a reference operand (and free at zero).
    Release(Operand),
    /// A call evaluated for its effect only (return value discarded).
    Call {
        callee: Callee,
        args: Vec<Operand>,
    },
    /// The `print`/`println` builtins, lowered to the host `print_*` imports. `ty` is the argument's
    /// interned type (selecting `$print_int`/`$print_char`/`$print_string`); `newline` appends `\n`.
    Print {
        arg: Operand,
        ty: TypeId,
        newline: bool,
    },
    /// No-op; left behind by passes that delete statements without renumbering.
    Nop,
}

/// How a block transfers control. Every block ends in exactly one terminator.
#[derive(Debug, Clone, Default)]
pub enum Terminator {
    Goto(BlockId),
    /// Two-way branch on a boolean operand.
    If {
        cond: Operand,
        then_blk: BlockId,
        else_blk: BlockId,
    },
    /// Multi-way branch (lowers to `br_table`): integer `value` matched against `targets`, falling
    /// through to `default`.
    Switch {
        value: Operand,
        targets: Vec<(i64, BlockId)>,
        default: BlockId,
    },
    Return(Option<Operand>),
    /// Completes the enclosing async task (`$dream_complete`) in a poll function. Used only by the
    /// async coroutine transform; synchronous functions use [`Terminator::Return`].
    AsyncComplete(Option<Operand>),
    /// Statically unreachable (e.g. after a diverging call); the placeholder default.
    #[default]
    Unreachable,
}

impl Terminator {
    /// The successor blocks of this terminator, for CFG traversal.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Goto(b) => vec![*b],
            Terminator::If { then_blk, else_blk, .. } => vec![*then_blk, *else_blk],
            Terminator::Switch { targets, default, .. } => {
                let mut s: Vec<BlockId> = targets.iter().map(|(_, b)| *b).collect();
                s.push(*default);
                s
            }
            Terminator::Return(_) | Terminator::AsyncComplete(_) | Terminator::Unreachable => vec![],
        }
    }
}

/// An assignable location.
#[derive(Debug, Clone)]
pub enum Place {
    Local(Local),
    Global(Global),
    /// `base.field` — `field` is the resolved field index.
    Field { base: Local, field: usize },
    /// `base[index]`.
    Index { base: Local, index: Box<Operand> },
}

/// A readable value: a local/global read or a constant. (All complex computation is an [`Rvalue`].)
#[derive(Debug, Clone)]
pub enum Operand {
    Copy(Place),
    Const(Const),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    /// A 32-bit integer literal (`int`/`uint`/`byte` — anything that lowers to `i32`).
    Int(i64),
    /// A 64-bit integer literal (`long`/`ulong`), kept distinct from [`Const::Int`] so the backend
    /// emits `i64.const` rather than truncating to `i32.const`.
    Long(i64),
    /// A 64-bit float literal (`double`), emitted as `f64.const`.
    Float(f64),
    /// A 32-bit float literal (`float`), kept distinct from [`Const::Float`] so the backend emits
    /// `f32.const` rather than widening to `f64.const`.
    F32(f32),
    Bool(bool),
    Char(char),
    /// An interned string; the backend resolves the pointer.
    Str(String),
    /// The null pointer.
    Null,
}

/// The right-hand side of an assignment: any computation producing a single value.
#[derive(Debug, Clone)]
pub enum Rvalue {
    Use(Operand),
    Binary(BinOp, Operand, Operand),
    Unary(UnOp, Operand),
    /// `string.len()` via a runtime `$strlen` scan.
    StrLen(Operand),
    /// `string.char_at(i)` via the runtime `$char_at` helper: `.0` is the string, `.1` the index.
    CharAt(Operand, Operand),
    /// `Array.new<T>(len)` — allocate a zero-initialized `T[]` block of a runtime length.
    ArrayNew { elem_ty: TypeId, len: Operand },
    /// The object-protocol `x.hash_code()` — dispatch on the operand's static type to a hash helper.
    HashCode(Operand),
    /// The object-protocol `x.to_string()` — dispatch on the operand's static type to a formatter.
    ToString(Operand),
    /// String concatenation `a + b` via the runtime `$concat_strings` (both operands are strings).
    Concat(Operand, Operand),
    /// `EnumValue.name()` — the operand's discriminant mapped to its interned variant-name string.
    /// `arms` is `(discriminant, variant name)`; an unmatched value produces the empty string.
    EnumName {
        value: Operand,
        arms: Vec<(i64, String)>,
    },
    /// A direct call returning a value.
    Call { callee: Callee, args: Vec<Operand> },
    /// An indirect call through a function-pointer operand.
    IndirectCall { target: Operand, args: Vec<Operand> },
    /// A first-class reference to a (possibly monomorphized) function, materialized as its index in
    /// the module's function table. Used when a function name is taken as a value (`let f = foo;`).
    FuncRef(Callee),
    /// Allocate and construct a struct instance. `ty` is the constructed value's interned type (the
    /// layout key, distinguishing generic instances); `def` tags the allocation. When `ctor` is
    /// `Some`, `args` are the user constructor's arguments (the backend allocates, zeroes, then calls
    /// `ctor(this, args)`); when `None`, `args` initialize the fields positionally.
    New {
        def: DefId,
        ty: TypeId,
        ctor: Option<DefId>,
        args: Vec<Operand>,
    },
    /// Construct a union variant. `ty` is the union's interned type (the layout key).
    UnionNew {
        def: DefId,
        ty: TypeId,
        variant: usize,
        args: Vec<Operand>,
    },
    /// Allocate an array literal of `elem_ty` from the given element operands.
    ArrayLit { elem_ty: TypeId, elems: Vec<Operand> },
    /// The stored length of an array.
    ArrayLen(Operand),
    /// A numeric/object coercion. Carries `(value, from_ty, to_ty)`; the source type is captured at
    /// lowering time so later constant propagation (which can replace the value with a bare `Const`
    /// that no longer distinguishes `int`/`uint`/`byte`) cannot lose the signedness needed to pick the
    /// correct widening/narrowing instruction.
    Cast(Operand, TypeId, TypeId),
    /// The active-variant discriminant of a union value (the `i32` at offset 0). Used to drive a
    /// `match` on union variants.
    Discriminant(Operand),
    /// Reads one payload field of a union variant: `base` is the union pointer, `ty` its interned
    /// union type (the layout key), `variant` the discriminant, and `field` the payload field index.
    /// The backend resolves the byte offset + load width from the union layout. Only sound in an arm
    /// already known (by discriminant dispatch) to hold this variant.
    UnionField {
        base: Operand,
        ty: TypeId,
        variant: usize,
        field: usize,
    },
    /// A runtime type test `value is T`: compares the boxed value's `$object_tag` against the tag of
    /// `TypeId`. Yields `bool`.
    IsType(Operand, TypeId),
}

/// A resolved call target carried into MIR. The backend derives the emitted symbol from
/// `(def, args)`; `ret` is the concrete return type at this site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callee {
    pub def: DefId,
    pub args: Vec<TypeId>,
    pub ret: TypeId,
}

/// Identity of a function/instance for the call graph: its def plus the concrete type-args of the
/// monomorphized instance (empty for non-generic functions), matching `MirFunction::{def, instance}`
/// and `Callee::{def, args}`.
type FnKey = (DefId, Vec<TypeId>);

/// Records every callable this rvalue statically references (direct calls, first-class function
/// refs, and user constructors) into `out`.
fn rvalue_callees(rv: &Rvalue, out: &mut Vec<FnKey>) {
    match rv {
        Rvalue::Call { callee, .. } | Rvalue::FuncRef(callee) => {
            out.push((callee.def, callee.args.clone()))
        }
        Rvalue::New { ctor: Some(ctor), .. } => out.push((*ctor, vec![])),
        _ => {}
    }
}

/// Removes functions unreachable from the module's entry points. The analyzer emits an [`HFunction`]
/// for *every* fully-lowerable function, including unused standard-prelude helpers (`List`, `Map`,
/// `JsonValue`, …) that are merged into every program; carrying them into the module would force the
/// assembler to resolve dead code that may reference runtime pieces the MIR backend has not wired yet.
///
/// Reachability starts from `main` and the synthesized global initializer and follows direct calls,
/// `FuncRef`s, and constructors. An `IndirectCall` has no static target, but its only possible
/// targets are functions whose address was taken by a `FuncRef` in reachable code — which the
/// `FuncRef` edges already keep — so the result stays sound.
pub fn prune_unreachable(mir: &mut Mir) {
    use std::collections::{HashMap, HashSet};

    let index: HashMap<FnKey, usize> = mir
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.def, f.instance.clone()), i))
        .collect();

    // `<Type>_del`/`<Type>_to_string` are invoked only by the generated RC runtime (the release
    // helpers and `$print_object`), never by a normal call edge, so reachability tracks them by name
    // for every type that is *live* — constructed (`New`/`UnionNew`) or printed — plus, transitively,
    // the types of its (reference) fields, whose release/print the runtime chains into.
    let by_name: HashMap<&str, usize> =
        mir.functions.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();

    let mut reachable: HashSet<usize> = HashSet::new();
    let mut live_types: HashSet<TypeId> = HashSet::new();
    let mut type_worklist: Vec<TypeId> = Vec::new();
    let mut worklist: Vec<usize> = mir
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| f.name == "main" || f.name == lower::INIT_FN_NAME)
        .map(|(i, _)| i)
        .collect();

    loop {
        while let Some(idx) = worklist.pop() {
            if !reachable.insert(idx) {
                continue;
            }
            let mut callees = Vec::new();
            for block in &mir.functions[idx].blocks {
                for stmt in &block.stmts {
                    match stmt {
                        Statement::Call { callee, .. } => {
                            callees.push((callee.def, callee.args.clone()))
                        }
                        Statement::Assign(_, rv) => {
                            rvalue_callees(rv, &mut callees);
                            match rv {
                                Rvalue::New { ty, .. } | Rvalue::UnionNew { ty, .. } => {
                                    type_worklist.push(*ty)
                                }
                                _ => {}
                            }
                        }
                        Statement::Print { ty, .. } => type_worklist.push(*ty),
                        _ => {}
                    }
                }
            }
            for key in callees {
                if let Some(&target) = index.get(&key) {
                    if !reachable.contains(&target) {
                        worklist.push(target);
                    }
                }
            }
        }

        // Expand the live-type frontier: keep each type's destructor/`to_string` and recurse into its
        // fields. Any newly-kept function is pushed back so its own body is walked; the outer loop
        // reaches a fixpoint once the function worklist drains and no new type is discovered.
        while let Some(ty) = type_worklist.pop() {
            if !live_types.insert(ty) {
                continue;
            }
            let mut field_tys = Vec::new();
            let mut names = Vec::new();
            if let Some(l) = mir.layouts.structs.get(&ty) {
                names.push(l.name.clone());
                field_tys.extend(l.fields.iter().map(|f| f.ty));
            }
            if let Some(l) = mir.layouts.unions.get(&ty) {
                names.push(l.name.clone());
                field_tys.extend(l.variants.iter().flat_map(|v| v.fields.iter().map(|f| f.ty)));
            }
            for name in names {
                for sym in [format!("{}_del", name), format!("{}_to_string", name)] {
                    if let Some(&idx) = by_name.get(sym.as_str()) {
                        if !reachable.contains(&idx) {
                            worklist.push(idx);
                        }
                    }
                }
            }
            type_worklist.extend(field_tys);
        }
        if worklist.is_empty() {
            break;
        }
    }
    drop(by_name);

    let mut keep = reachable.into_iter().collect::<Vec<_>>();
    keep.sort_unstable();
    let mut kept = Vec::with_capacity(keep.len());
    for (i, f) in std::mem::take(&mut mir.functions).into_iter().enumerate() {
        if keep.binary_search(&i).is_ok() {
            kept.push(f);
        }
    }
    mir.functions = kept;
}

#[cfg(test)]
mod tests {
    use crate::hir::{Binding, HExpr, HExprKind, HFunction, HParam, HStmt, LocalId};
    use crate::mir::lower::lower_function;
    use crate::mir::passes::PassManager;
    use crate::types::{DefKind, TypeCtx};

    /// Exercises the whole middle/back-end: build typed HIR, lower to a MIR CFG, run the
    /// optimization pipeline, and emit WAT.
    #[test]
    fn hir_to_mir_to_optimized_wat() {
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Function, "add", vec![]);
        let int = ctx.interner.int();

        // fun add(a: int, b: int): int { return a + b; }
        let func = HFunction {
            def,
            name: "add".into(),
            instance: vec![],
            params: vec![
                HParam { local: LocalId(0), name: "a".into(), ty: int },
                HParam { local: LocalId(1), name: "b".into(), ty: int },
            ],
            ret: int,
            locals: vec![],
            is_async: false,
            body: vec![HStmt::Return(Some(HExpr::new(
                int,
                HExprKind::Binary {
                    op: crate::hir::BinOp::Add,
                    lhs: Box::new(HExpr::new(int, HExprKind::Var(Binding::Local(LocalId(0))))),
                    rhs: Box::new(HExpr::new(int, HExprKind::Var(Binding::Local(LocalId(1))))),
                },
            )))],
        };

        let mut mir = lower_function(&func, &ctx.interner);
        PassManager::default_pipeline().run(&mut mir, &ctx.interner);
        let wat = super::emit::emit_function(&mir, &ctx.interner);
        assert!(wat.contains("(func $add"));
        assert!(wat.contains("i32.add"), "pipeline output:\n{}", wat);
        assert!(wat.contains("(return)"));
    }
}
