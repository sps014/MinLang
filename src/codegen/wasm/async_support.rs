//! Async/await lowering and the cooperative scheduler runtime.
//!
//! An `async fun` is compiled to two WebAssembly functions:
//!   * a thin **constructor** (`$name`) that allocates a `Future` task frame, stores the
//!     parameters into the frame, eagerly enqueues the first poll, and returns the frame pointer;
//!   * a **poll** function (`$poll_<name>`) - a resumable state machine that runs the straight-line
//!     code between `await`s, saving/restoring all locals across each suspension point.
//!
//! A small scheduler (ready queue + virtual-clock timer queue) drives the polls. It is fully
//! self-contained inside the module: the exported `main` wrapper runs the loop to completion, so
//! native (`wasmtime`) and browser hosts need no extra wiring for the built-in `sleep`/combinators.
//! `await`ing a JS promise (`extern async`) is bridged by `dream.js` via the exported
//! `__dream_resolve`, which resolves a host `Future` the import created.

use std::collections::HashMap;
use std::io::Error;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, StatementNode};
use crate::syntax::text::indented_text_writer::IndentedTextWriter;
use super::WasmGenerator;

// `Future` heap-block field offsets (relative to the data pointer). The block is allocated via
// `$dream_new_future` and zeroed; task frames append a saved-locals region after `F_SLOTS`.
const F_STATE: i32 = 0;       // resume-state index for the poll's `br_table`
const F_STATUS: i32 = 4;      // 0 = pending, 1 = ready
const F_RESULT: i32 = 8;      // resolved value (i32-compatible)
const F_POLL: i32 = 12;       // poll function table index (-1 for host/combinator futures)
const F_WAKER: i32 = 16;      // parent future awaiting this one (0 = none)
const F_AWAITING: i32 = 20;   // child future currently awaited (read on resume)
const F_KIND: i32 = 24;       // 0 task, 1 host, 2 all, 3 any/race
const F_CHILDREN: i32 = 28;   // combinator: child-future array pointer
const F_COUNT: i32 = 32;      // combinator: number of children
const F_REMAINING: i32 = 36;  // combinator: children left to settle
const F_RESULTS: i32 = 40;    // combinator `all`: results array pointer
const F_NEXT: i32 = 44;       // ready-queue / timer-queue link
const F_QUEUED: i32 = 48;     // already-in-ready-queue flag
const F_DUE: i32 = 52;        // host timer: virtual due time
const F_SLOTS: i32 = 56;      // start of the saved-locals region (8 bytes per slot)

const KIND_TASK: i32 = 0;
const KIND_HOST: i32 = 1;
const KIND_ALL: i32 = 2;
const KIND_ANY: i32 = 3;

const SLOT_SIZE: i32 = 8;

/// What to do at the start of a resumed segment with the value the preceding `await` produced.
enum Resume {
    /// First segment: nothing to bind.
    None,
    /// `let x = await e;` - store the awaited result into local `x`.
    BindLocal(String),
    /// `await e;` - discard the awaited result.
    Discard,
    /// `return await e;` - complete the future with the awaited result and return.
    ReturnAwaited,
}

/// How a segment ends.
enum SegEnd<'a> {
    /// Suspend on `child`, resuming into state `next`.
    Suspend(&'a ExpressionNode<'a>, usize),
    /// Fall off the end of the body: complete the (void) future.
    CompleteVoid,
}

struct Segment<'a> {
    resume: Resume,
    plain: Vec<&'a StatementNode<'a>>,
    end: SegEnd<'a>,
}

impl<'a> WasmGenerator<'a> {
    /// True for a `Future`-typed value name (`Future_<inner>`), which the codegen treats as a
    /// plain `i32` handle (never ref-counted).
    pub fn is_future_type(name: &str) -> bool {
        name.starts_with("Future_")
    }

    /// The WAT store instruction for a wasm value type.
    fn slot_store(wt: &str) -> &'static str {
        match wt {
            "f64" => "f64.store",
            "f32" => "f32.store",
            _ => "i32.store",
        }
    }

    /// The WAT load instruction for a wasm value type.
    fn slot_load(wt: &str) -> &'static str {
        match wt {
            "f64" => "f64.load",
            "f32" => "f32.load",
            _ => "i32.load",
        }
    }

    /// Collects the saved-frame slots (params + locals) of an async function as
    /// `(name, wasm_type)`, sorted by name for a stable layout, plus a name -> byte offset map.
    fn async_slots(&self, function: &FunctionNode<'a>, func_name: &str) -> Result<(Vec<(String, String)>, HashMap<String, i32>), Error> {
        let locals = self.get_local_variables(self.symbol_map.get(func_name).unwrap())?;
        let mut entries: Vec<(String, String)> = locals.into_iter()
            .map(|(name, ty)| (name, WasmGenerator::get_wasm_type_from(self.resolve_type(&ty.get_type())).unwrap_or_else(|_| "i32".to_string())))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut offsets = HashMap::new();
        for (i, (name, _)) in entries.iter().enumerate() {
            offsets.insert(name.clone(), F_SLOTS + (i as i32) * SLOT_SIZE);
        }
        Ok((entries, offsets))
    }

    /// Emits the constructor + poll function for one `async fun`.
    pub fn build_async_function(&mut self, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let func_name = self.ctx.current_mangled_name.clone().unwrap_or_else(|| function.name.text.clone());
        let (slots, offsets) = self.async_slots(function, &func_name)?;
        let frame_size = F_SLOTS + (slots.len() as i32) * SLOT_SIZE;
        let poll_idx = *self.ctx.poll_indices.get(&func_name)
            .unwrap_or_else(|| panic!("no poll index for async function {}", func_name));

        // ---- Constructor: allocate frame, store params, spawn, return future pointer ----
        writer.write(&format!("(func ${}", func_name));
        for p in function.parameters.iter() {
            self.build_parameter(p, writer)?;
        }
        writer.write(" (result i32)");
        writer.write(" (local $self i32)");
        writer.write_line("");
        writer.indent();

        writer.write_line(&format!("i32.const {}", frame_size));
        writer.write_line(&format!("i32.const {}", poll_idx));
        writer.write_line(&format!("i32.const {}", KIND_TASK));
        writer.write_line("call $dream_new_future");
        writer.write_line("local.set $self");
        for p in function.parameters.iter() {
            let off = offsets.get(&p.name.text).copied().unwrap_or(F_SLOTS);
            let wt = WasmGenerator::get_wasm_type_from(self.resolve_type(&p.type_.get_type()))?;
            writer.write_line("local.get $self");
            writer.write_line(&format!("local.get ${}", p.name.text));
            writer.write_line(&format!("{} offset={}", Self::slot_store(&wt), off));
        }
        writer.write_line("local.get $self");
        writer.write_line("call $dream_enqueue");
        writer.write_line("local.get $self");
        writer.unindent();
        writer.write_line(")");

        // ---- Poll: the resumable state machine ----
        writer.write(&format!("(func $poll_{} (param $self i32) (result i32)", func_name));
        // User parameters become locals (restored from the frame each poll).
        for p in function.parameters.iter() {
            let wt = WasmGenerator::get_wasm_type_from(self.resolve_type(&p.type_.get_type()))?;
            writer.write(&format!(" (local ${} {})", p.name.text, wt));
        }
        // User locals (also registers the symbol lookup used by statement/expression codegen).
        self.build_local_variable(function, writer)?;
        // Scratch locals mirroring `build_function`.
        writer.write(" (local $scratch_ptr i32)");
        writer.write(" (local $scratch_addr i32)");
        writer.write(" (local $scratch_double f64)");
        writer.write(" (local $scratch_float f32)");
        writer.write(" (local $scratch_len i32)");
        writer.write(" (local $scratch_arr i32)");
        writer.write(" (local $scratch_switch i32)");
        writer.write(" (local $scratch_coalesce i32)");
        for i in 0..Self::CTOR_BASE_POOL {
            writer.write(&format!(" (local $ctor_base{} i32)", i));
        }
        for i in 0..Self::TMP_POOL {
            writer.write(&format!(" (local $tmp{} i32)", i));
        }
        writer.write_line("");
        writer.indent();

        // Restore every slot into its local before dispatching.
        for (name, wt) in slots.iter() {
            let off = offsets[name];
            writer.write_line("local.get $self");
            writer.write_line(&format!("{} offset={}", Self::slot_load(wt), off));
            writer.write_line(&format!("local.set ${}", name));
        }

        let segments = self.split_async_segments(function);
        self.ctx.current_async_self = Some("self".to_string());
        self.emit_async_segments(&segments, &slots, &offsets, function, writer)?;
        self.ctx.current_async_self = None;

        writer.unindent();
        writer.write_line(")");
        Ok(())
    }

    /// Splits an async function body into resumable segments at each top-level `await`.
    fn split_async_segments(&self, function: &FunctionNode<'a>) -> Vec<Segment<'a>> {
        let mut segs: Vec<Segment<'a>> = Vec::new();
        let mut resume = Resume::None;
        let mut plain: Vec<&'a StatementNode<'a>> = Vec::new();

        for stmt in function.body.iter() {
            match stmt {
                StatementNode::Declaration(tok, _, ExpressionNode::Await(child), _) => {
                    let next = segs.len() + 1;
                    segs.push(Segment { resume: std::mem::replace(&mut resume, Resume::BindLocal(tok.text.clone())), plain: std::mem::take(&mut plain), end: SegEnd::Suspend(child, next) });
                }
                StatementNode::AwaitStmt(child) => {
                    let next = segs.len() + 1;
                    segs.push(Segment { resume: std::mem::replace(&mut resume, Resume::Discard), plain: std::mem::take(&mut plain), end: SegEnd::Suspend(child, next) });
                }
                StatementNode::Return(Some(ExpressionNode::Await(child))) => {
                    let next = segs.len() + 1;
                    segs.push(Segment { resume: std::mem::replace(&mut resume, Resume::ReturnAwaited), plain: std::mem::take(&mut plain), end: SegEnd::Suspend(child, next) });
                }
                other => plain.push(other),
            }
        }
        segs.push(Segment { resume, plain, end: SegEnd::CompleteVoid });
        segs
    }

    /// Emits the state-machine dispatch (`br_table`) and every segment body.
    fn emit_async_segments(&mut self, segments: &[Segment<'a>], slots: &[(String, String)], offsets: &HashMap<String, i32>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        let n = segments.len();
        // Open nested blocks, outermost first (`$async_seg{n-1}` .. `$async_seg0`).
        for k in (0..n).rev() {
            writer.write_line(&format!("(block $async_seg{}", k));
            writer.indent();
        }
        // Dispatch on the saved state.
        writer.write_line("local.get $self");
        writer.write_line(&format!("i32.load offset={}", F_STATE));
        let labels: Vec<String> = (0..n).map(|k| format!("$async_seg{}", k)).collect();
        writer.write_line(&format!("br_table {}", labels.join(" ")));
        // Close each block and emit its segment body.
        for (k, seg) in segments.iter().enumerate() {
            writer.unindent();
            writer.write_line(")");
            self.emit_one_segment(seg, slots, offsets, function, writer)?;
        }
        Ok(())
    }

    fn emit_one_segment(&mut self, seg: &Segment<'a>, slots: &[(String, String)], offsets: &HashMap<String, i32>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        // Resume binding for the value the preceding `await` produced.
        match &seg.resume {
            Resume::None => {}
            Resume::BindLocal(name) => {
                writer.write_line("local.get $self");
                writer.write_line(&format!("i32.load offset={}", F_AWAITING));
                writer.write_line(&format!("i32.load offset={}", F_RESULT));
                writer.write_line(&format!("local.set ${}", name));
            }
            Resume::Discard => {}
            Resume::ReturnAwaited => {
                writer.write_line("local.get $self");
                writer.write_line("local.get $self");
                writer.write_line(&format!("i32.load offset={}", F_AWAITING));
                writer.write_line(&format!("i32.load offset={}", F_RESULT));
                writer.write_line("call $dream_complete");
                writer.write_line("i32.const 0");
                writer.write_line("return");
                return Ok(());
            }
        }

        for stmt in seg.plain.iter() {
            self.build_statement(stmt, function, writer)?;
        }

        match &seg.end {
            SegEnd::Suspend(child, next) => self.emit_suspend(child, *next, slots, offsets, function, writer)?,
            SegEnd::CompleteVoid => {
                writer.write_line("local.get $self");
                writer.write_line("i32.const 0");
                writer.write_line("call $dream_complete");
                writer.write_line("i32.const 0");
                writer.write_line("return");
            }
        }
        Ok(())
    }

    /// Emits a suspension point: evaluate the child future, record it, save all locals, register
    /// the current task as its waker, and return `Pending`.
    fn emit_suspend(&mut self, child: &ExpressionNode<'a>, next_state: usize, slots: &[(String, String)], offsets: &HashMap<String, i32>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        self.build_expression(child, &"int".to_string(), function, writer)?;
        writer.write_line("local.set $scratch_ptr");
        // AWAITING = child
        writer.write_line("local.get $self");
        writer.write_line("local.get $scratch_ptr");
        writer.write_line(&format!("i32.store offset={}", F_AWAITING));
        // STATE = next
        writer.write_line("local.get $self");
        writer.write_line(&format!("i32.const {}", next_state));
        writer.write_line(&format!("i32.store offset={}", F_STATE));
        // Save all locals into the frame.
        for (name, wt) in slots.iter() {
            let off = offsets[name];
            writer.write_line("local.get $self");
            writer.write_line(&format!("local.get ${}", name));
            writer.write_line(&format!("{} offset={}", Self::slot_store(wt), off));
        }
        // Register as waker and suspend.
        writer.write_line("local.get $self");
        writer.write_line("local.get $scratch_ptr");
        writer.write_line("call $dream_await");
        writer.write_line("i32.const 0");
        writer.write_line("return");
        Ok(())
    }

    /// Emits an async intrinsic call (`sleep`/`all`/`any`/`race`), leaving a `Future` pointer on
    /// the stack. These are compiler-known because their signatures are generic over `Future<T>`.
    pub fn build_async_intrinsic_call(&mut self, name: &str, args: &Vec<ExpressionNode<'a>>, function: &FunctionNode<'a>, writer: &mut IndentedTextWriter) -> Result<(), Error> {
        match name {
            "sleep" => {
                // sleep(ms): allocate a host future and arm a virtual-clock timer.
                self.build_expression(&args[0], &"int".to_string(), function, writer)?;
                writer.write_line("local.set $scratch_len");
                writer.write_line(&format!("i32.const {}", F_SLOTS));
                writer.write_line("i32.const -1");
                writer.write_line(&format!("i32.const {}", KIND_HOST));
                writer.write_line("call $dream_new_future");
                writer.write_line("local.tee $scratch_arr");
                writer.write_line("local.get $scratch_len");
                writer.write_line("call $dream_set_timer");
                writer.write_line("local.get $scratch_arr");
            }
            "all" | "any" | "race" => {
                let arg_type = self.infer_expression_type(&args[0], function)?;
                self.build_expression(&args[0], &arg_type, function, writer)?;
                let runtime = if name == "all" { "$dream_all" } else { "$dream_any" };
                writer.write_line(&format!("call {}", runtime));
            }
            _ => {}
        }
        Ok(())
    }

    /// Emits the scheduler runtime (queue/timer globals, poll-dispatch type, and helper
    /// functions). Called once when the program contains any `async fun`.
    pub fn build_async_runtime(&self, writer: &mut IndentedTextWriter) {
        let tag_array = super::object::TAG_ARRAY;
        let runtime = format!(r#"(type $dream_poll_t (func (param i32) (result i32)))
(global $rq_head (mut i32) (i32.const 0))
(global $rq_tail (mut i32) (i32.const 0))
(global $timer_head (mut i32) (i32.const 0))
(global $vclock (mut i32) (i32.const 0))
(func $dream_new_future (param $size i32) (param $poll i32) (param $kind i32) (result i32)
    (local $p i32)
    local.get $size
    i32.const 0
    call $malloc
    local.set $p
    local.get $p
    i32.const 0
    local.get $size
    memory.fill
    local.get $p
    local.get $poll
    i32.store offset={F_POLL}
    local.get $p
    local.get $kind
    i32.store offset={F_KIND}
    local.get $p
)
(func $dream_enqueue (param $f i32)
    local.get $f
    i32.eqz
    br_if 0
    local.get $f
    i32.load offset={F_QUEUED}
    br_if 0
    local.get $f
    i32.const 1
    i32.store offset={F_QUEUED}
    local.get $f
    i32.const 0
    i32.store offset={F_NEXT}
    global.get $rq_tail
    i32.eqz
    (if
        (then
            local.get $f
            global.set $rq_head
            local.get $f
            global.set $rq_tail
        )
        (else
            global.get $rq_tail
            local.get $f
            i32.store offset={F_NEXT}
            local.get $f
            global.set $rq_tail
        )
    )
)
(func $dream_complete (param $f i32) (param $res i32)
    (local $w i32)
    local.get $f
    local.get $res
    i32.store offset={F_RESULT}
    local.get $f
    i32.const 1
    i32.store offset={F_STATUS}
    local.get $f
    i32.load offset={F_WAKER}
    local.set $w
    local.get $w
    i32.eqz
    br_if 0
    local.get $w
    local.get $f
    call $dream_wake
)
(func $dream_wake (param $w i32) (param $child i32)
    local.get $w
    i32.load offset={F_KIND}
    i32.eqz
    (if
        (then
            local.get $w
            call $dream_enqueue
        )
        (else
            local.get $w
            local.get $child
            call $dream_combinator_progress
        )
    )
)
(func $dream_await (param $parent i32) (param $child i32)
    local.get $child
    local.get $parent
    i32.store offset={F_WAKER}
    local.get $child
    i32.load offset={F_STATUS}
    (if
        (then
            local.get $parent
            call $dream_enqueue
        )
    )
)
(func $dream_resolve (param $f i32) (param $res i32)
    local.get $f
    local.get $res
    call $dream_complete
)
(func $dream_set_timer (param $f i32) (param $delay i32)
    (local $due i32)
    (local $cur i32)
    (local $nxt i32)
    global.get $vclock
    local.get $delay
    i32.add
    local.set $due
    local.get $f
    local.get $due
    i32.store offset={F_DUE}
    global.get $timer_head
    i32.eqz
    (if
        (then
            local.get $f
            i32.const 0
            i32.store offset={F_NEXT}
            local.get $f
            global.set $timer_head
            return
        )
    )
    global.get $timer_head
    i32.load offset={F_DUE}
    local.get $due
    i32.gt_s
    (if
        (then
            local.get $f
            global.get $timer_head
            i32.store offset={F_NEXT}
            local.get $f
            global.set $timer_head
            return
        )
    )
    global.get $timer_head
    local.set $cur
    (block $done
        (loop $scan
            local.get $cur
            i32.load offset={F_NEXT}
            local.set $nxt
            local.get $nxt
            i32.eqz
            br_if $done
            local.get $nxt
            i32.load offset={F_DUE}
            local.get $due
            i32.gt_s
            br_if $done
            local.get $nxt
            local.set $cur
            br $scan
        )
    )
    local.get $f
    local.get $cur
    i32.load offset={F_NEXT}
    i32.store offset={F_NEXT}
    local.get $cur
    local.get $f
    i32.store offset={F_NEXT}
)
(func $dream_run_loop
    (local $f i32)
    (local $t i32)
    (block $alldone
        (loop $outer
            (block $drained
                (loop $drain
                    global.get $rq_head
                    local.set $f
                    local.get $f
                    i32.eqz
                    br_if $drained
                    local.get $f
                    i32.load offset={F_NEXT}
                    global.set $rq_head
                    global.get $rq_head
                    i32.eqz
                    (if
                        (then
                            i32.const 0
                            global.set $rq_tail
                        )
                    )
                    local.get $f
                    i32.const 0
                    i32.store offset={F_QUEUED}
                    local.get $f
                    i32.const 0
                    i32.store offset={F_NEXT}
                    local.get $f
                    local.get $f
                    i32.load offset={F_POLL}
                    call_indirect (type $dream_poll_t)
                    drop
                    br $drain
                )
            )
            global.get $timer_head
            i32.eqz
            br_if $alldone
            global.get $timer_head
            i32.load offset={F_DUE}
            global.set $vclock
            (block $timers_done
                (loop $tloop
                    global.get $timer_head
                    local.set $t
                    local.get $t
                    i32.eqz
                    br_if $timers_done
                    local.get $t
                    i32.load offset={F_DUE}
                    global.get $vclock
                    i32.gt_s
                    br_if $timers_done
                    local.get $t
                    i32.load offset={F_NEXT}
                    global.set $timer_head
                    local.get $t
                    i32.const 0
                    i32.store offset={F_NEXT}
                    local.get $t
                    i32.const 0
                    call $dream_complete
                    br $tloop
                )
            )
            br $outer
        )
    )
)
(func $dream_combinator_progress (param $w i32) (param $child i32)
    (local $n i32)
    (local $i i32)
    (local $arr i32)
    (local $c i32)
    local.get $w
    i32.load offset={F_KIND}
    i32.const {KIND_ALL}
    i32.eq
    (if
        (then
            local.get $w
            local.get $w
            i32.load offset={F_REMAINING}
            i32.const 1
            i32.sub
            i32.store offset={F_REMAINING}
            local.get $w
            i32.load offset={F_REMAINING}
            i32.eqz
            (if
                (then
                    local.get $w
                    i32.load offset={F_COUNT}
                    local.set $n
                    i32.const 4
                    local.get $n
                    i32.const 4
                    i32.mul
                    i32.add
                    i32.const {tag_array}
                    call $malloc
                    local.set $arr
                    local.get $arr
                    local.get $n
                    i32.store
                    i32.const 0
                    local.set $i
                    (block $fdone
                        (loop $f
                            local.get $i
                            local.get $n
                            i32.ge_s
                            br_if $fdone
                            local.get $w
                            i32.load offset={F_CHILDREN}
                            i32.const 4
                            i32.add
                            local.get $i
                            i32.const 4
                            i32.mul
                            i32.add
                            i32.load
                            local.set $c
                            local.get $arr
                            i32.const 4
                            i32.add
                            local.get $i
                            i32.const 4
                            i32.mul
                            i32.add
                            local.get $c
                            i32.load offset={F_RESULT}
                            i32.store
                            local.get $i
                            i32.const 1
                            i32.add
                            local.set $i
                            br $f
                        )
                    )
                    local.get $w
                    local.get $arr
                    i32.store offset={F_RESULTS}
                    local.get $w
                    local.get $arr
                    call $dream_complete
                )
            )
        )
        (else
            local.get $w
            i32.load offset={F_STATUS}
            i32.eqz
            (if
                (then
                    local.get $w
                    local.get $child
                    i32.load offset={F_RESULT}
                    call $dream_complete
                )
            )
        )
    )
)
(func $dream_all (param $arr i32) (result i32)
    (local $w i32)
    (local $n i32)
    (local $i i32)
    (local $c i32)
    local.get $arr
    i32.load
    local.set $n
    i32.const {F_SLOTS}
    i32.const -1
    i32.const {KIND_ALL}
    call $dream_new_future
    local.set $w
    local.get $w
    local.get $arr
    i32.store offset={F_CHILDREN}
    local.get $w
    local.get $n
    i32.store offset={F_COUNT}
    local.get $w
    local.get $n
    i32.store offset={F_REMAINING}
    local.get $n
    i32.eqz
    (if
        (then
            local.get $w
            local.get $arr
            call $dream_complete
            local.get $w
            return
        )
    )
    i32.const 0
    local.set $i
    (block $done
        (loop $reg
            local.get $i
            local.get $n
            i32.ge_s
            br_if $done
            local.get $arr
            i32.const 4
            i32.add
            local.get $i
            i32.const 4
            i32.mul
            i32.add
            i32.load
            local.set $c
            local.get $c
            local.get $w
            i32.store offset={F_WAKER}
            local.get $c
            i32.load offset={F_STATUS}
            (if
                (then
                    local.get $w
                    local.get $c
                    call $dream_combinator_progress
                )
            )
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $reg
        )
    )
    local.get $w
)
(func $dream_any (param $arr i32) (result i32)
    (local $w i32)
    (local $n i32)
    (local $i i32)
    (local $c i32)
    local.get $arr
    i32.load
    local.set $n
    i32.const {F_SLOTS}
    i32.const -1
    i32.const {KIND_ANY}
    call $dream_new_future
    local.set $w
    local.get $w
    local.get $arr
    i32.store offset={F_CHILDREN}
    local.get $w
    local.get $n
    i32.store offset={F_COUNT}
    local.get $w
    local.get $n
    i32.store offset={F_REMAINING}
    i32.const 0
    local.set $i
    (block $done
        (loop $reg
            local.get $i
            local.get $n
            i32.ge_s
            br_if $done
            local.get $arr
            i32.const 4
            i32.add
            local.get $i
            i32.const 4
            i32.mul
            i32.add
            i32.load
            local.set $c
            local.get $c
            local.get $w
            i32.store offset={F_WAKER}
            local.get $c
            i32.load offset={F_STATUS}
            (if
                (then
                    local.get $w
                    local.get $c
                    call $dream_combinator_progress
                )
            )
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $reg
        )
    )
    local.get $w
)
"#,
            F_POLL = F_POLL, F_KIND = F_KIND, F_QUEUED = F_QUEUED, F_NEXT = F_NEXT,
            F_RESULT = F_RESULT, F_STATUS = F_STATUS, F_WAKER = F_WAKER, F_DUE = F_DUE,
            F_CHILDREN = F_CHILDREN, F_COUNT = F_COUNT, F_REMAINING = F_REMAINING,
            F_RESULTS = F_RESULTS, F_SLOTS = F_SLOTS, KIND_ALL = KIND_ALL, KIND_ANY = KIND_ANY,
            tag_array = tag_array);
        writer.write_block(&runtime);
    }
}
