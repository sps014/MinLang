# 06 — Relooper & WAT Backend (`src/mir/relooper.rs`, `src/mir/emit.rs`)

The backend turns optimized MIR into WebAssembly text (WAT). The hard part is control flow: MIR is an
arbitrary (reducible) CFG, but WASM has **no `goto`** — only structured `block`/`loop`/`if` and
relative branches (`br`/`br_if`/`br_table`). The relooper bridges that gap.

## The two-layer backend

```mermaid
flowchart TD
    mir[Optimized MIR function] --> rl[relooper::reloop]
    rl --> shape["Shape tree\n(Simple / Loop / Multiple)"]
    shape --> emit[emit::emit_function]
    mir --> emit
    emit --> wat["WAT (func ...)"]
    emit -. reuses .-> rt["runtime / object / memory / string layers"]
```

- `relooper::reloop(func) -> Option<Shape>` recovers structured shapes.
- `emit::emit_program / emit_function` walks the function and writes WAT, consulting the type interner
  for WASM value types and reusing the existing runtime layers for heap layout and strings.

## The relooper

### Why it is needed

```mermaid
flowchart LR
    subgraph "CFG (MIR)"
      A --> B
      A --> C
      B --> D
      C --> D
      D --> B
    end
```

A CFG like this (a diamond whose join loops back) cannot be written directly in WASM. The relooper
discovers that `B → D → B` forms a loop and that `A` branches into two arms, and produces a tree of
**shapes** the emitter can translate to nested `block`/`loop`/`if`.

### Shapes — `Shape` enum

```rust
pub enum Shape {
    Simple   { block: BlockId,       next: Option<Box<Shape>> }, // one block, then the rest
    Loop     { inner: Box<Shape>,    next: Option<Box<Shape>> }, // cyclic region in a `loop`, then rest
    Multiple { handled: Vec<Shape>,  next: Option<Box<Shape>> }, // independent arms, then the join
}
```

### The algorithm (`Relooper::make`)

`make(entries, within, headers)` recursively builds the shape for the sub-CFG restricted to `within`,
entered at `entries`, where `headers` are the entry blocks of *enclosing* loops:

```mermaid
flowchart TD
    start{"how many entries?"}
    start -->|"1 entry, not a loop header"| simple["Simple{block}; recurse on its successors"]
    start -->|"entries can reach themselves"| loop["Loop{inner}; headers ∪= entries; recurse"]
    start -->|"multiple independent entries"| multiple["Multiple{handled arms}; recurse on join"]
```

The single subtle point — and the bug that was fixed during development — is **back-edges**. Inside a
`Loop`, an edge back to the loop header is a `continue`, *not* forward control flow. So `succs` and
`reach` **filter out `headers`**: they never traverse back into an enclosing loop's entry. Without this
filter, `make_loop` re-detects the header as a fresh loop entry and recurses forever (stack overflow).
This is why `headers: &BTreeSet<BlockId>` is threaded through every recursive call.

Because Dream's surface syntax only generates reducible CFGs, `reloop` always returns `Some`. It is
typed `Option<Shape>` so an irreducible graph fails loudly rather than miscompiling.

## The emitter (`emit.rs`)

### Today: a dispatch loop

The current emitter does **not** yet consume the relooper shapes. Instead it uses a
**labeled-block dispatch loop**: a `$blockidx` local holds "which block to run next", an outer `loop`
wraps a `br_table` that jumps to the current block's code, each block ends by setting `$blockidx` and
`br`-ing back to the dispatch, and `Return` exits. This is correct for *any* reducible CFG and was the
fastest way to get a working backend.

```wat
(func $f (param ...) (result ...)
  (local $blockidx i32)
  (loop $dispatch
    (block $bb2 (block $bb1 (block $bb0
      (br_table $bb0 $bb1 $bb2 (local.get $blockidx))))
      ;; bb0 code ... (local.set $blockidx (...)) (br $dispatch)
    ) ;; bb1 ...
  )
)
```

The relooper output is the basis for the **planned refinement**: emit idiomatic nested
`block`/`loop`/`if` (smaller, faster, friendlier to the WASM engine's own optimizer) instead of the
dispatch loop. The shape tree is already produced and tested; wiring `emit` to walk it is the next
backend task.

### Statements, operands, types

- `wasm_ty(TypeId)` maps interned types to WASM value types: `i32` for ints/bools/chars/refs (pointers),
  `i64` for longs, `f32`/`f64` for floats. Reference types are `i32` pointers into linear memory.
- `binop_instr` picks the instruction from `(BinOp, operand type)` — e.g. `i32.add`, `f64.mul`,
  `i32.lt_s` vs `i32.lt_u` based on signedness.
- Operands lower trivially: `Const` → `i32.const`/`f64.const`/…; `Copy(Place::Local)` → `local.get`.

### Runtime integration points (the `;; TODO` markers)

Three families of operations need the existing runtime layers and are currently stubbed:

```mermaid
flowchart LR
    subgraph "MIR construct"
      f["Place::Field / Place::Index"]
      n["Rvalue::New / UnionNew / ArrayLit"]
      s["Const::Str"]
    end
    subgraph "Existing runtime layer to reuse"
      mem["codegen::wasm::memory\n(layout, field offsets)"]
      obj["codegen::wasm::object\n(constructors, vtables)"]
      str["string interning / data segments"]
    end
    f -->|;; TODO layout| mem
    n -->|;; TODO layout| obj
    s -->|;; TODO strings| str
```

- **Field/index access** needs struct/array **layout** (field offsets, element stride, header size).
  That lives in the memory/layout layer used by the legacy backend; the emitter must compute
  `base + offset` loads/stores from it.
- **Allocation/construction** (`New`, `UnionNew`, `ArrayLit`) needs the object/allocation layer
  (allocate, set header/refcount, run the constructor).
- **String constants** need the string-interning/data-segment layer so identical literals share one
  pointer.

Wiring these is **Phase 5 runtime integration** ([09-migration-status.md](./09-migration-status.md)).
The contract is: reuse the *same* layout/alloc/string code the legacy backend uses, so the two
backends agree byte-for-byte and the determinism test stays green during the switch.

## Determinism in the backend

The emitter must be a pure function of the MIR. Iterate `Vec`s in order; never iterate a
`std::HashMap`. Any lookup tables introduced (string pool, function index map) must be `IndexMap`/
`BTreeMap` so two runs emit identical WAT. The `codegen_is_deterministic` e2e test enforces this.
