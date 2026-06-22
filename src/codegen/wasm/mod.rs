use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::io::Error;

use crate::syntax::nodes::Type;
use crate::syntax::syntax_tree::SyntaxTree;
use crate::semantics::analyzer::SemanticInfo;
use crate::semantics::function_table::FunctionTable;
use crate::semantics::symbol_table::SymbolTable;
use crate::codegen::CodeGenerator;

pub mod expression;
pub mod module;
pub mod statement;
pub mod strings;
pub mod utils;
pub mod memory;
pub mod object;

/// Byte size of the universal heap-block header: `[size:i32][tag:i32][ref_count:i32]`.
/// Allocated pointers point at `data` (block_start + HEAP_HEADER_SIZE).
pub const HEAP_HEADER_SIZE: usize = 12;

/// Mutable working state accumulated while lowering a module to WAT. Separated from the
/// borrowed, read-only inputs on [`WasmGenerator`] so the generator cleanly distinguishes
/// "what we read" (syntax tree, semantic tables) from "what we mutate" during emission.
#[allow(dead_code)]
pub struct CodegenContext {
    // key 1: function name, key 2: parameter name
    pub combined_symbol_lookup: HashMap<String, HashMap<String, Type>>,
    pub strings: HashMap<String, usize>,
    /// Runtime-only string literals (e.g. "true", "null", struct labels) interned by the object
    /// protocol; maps raw (already unquoted) content -> data-segment offset.
    pub runtime_strings: HashMap<String, usize>,
    pub next_string_offset: usize,
    pub loop_counter: usize,
    /// Stack of active loops as `(loop_id, optional_label)` so labeled `break`/`continue` can
    /// target an enclosing loop by name.
    pub loop_stack: Vec<(usize, Option<String>)>,
    /// A label parsed via `label:` that the next loop construct should adopt.
    pub pending_loop_label: Option<String>,
    /// Active generic parameter -> concrete type bindings while emitting a monomorphized
    /// generic function body (empty when not inside one).
    pub current_generic_bindings: HashMap<String, String>,
    pub current_mangled_name: Option<String>,
    /// Stable function-table index assigned to each indexable (non-generic) top-level function.
    /// Used to lower first-class function values and `call_indirect`.
    pub function_indices: HashMap<String, usize>,
    /// Current nesting depth of heap constructors (struct instantiations / array literals).
    /// Each level borrows a distinct `$ctor_base{depth}` local to hold its allocation pointer
    /// across sub-expression evaluation, so nested literals (`[P{...}]`, `Box<Box<int>>`) do
    /// not clobber each other's base pointer.
    pub alloc_depth: usize,
    /// Number of `$tmp{n}` temp locals currently held live. Owned-reference call arguments are
    /// `local.tee`'d into the next free `$tmp{n}` so they can be released after the call; the
    /// counter advances while a slot is held and is restored once the call's temps are released.
    pub tmp_depth: usize,
}

impl CodegenContext {
    fn new() -> Self {
        Self {
            combined_symbol_lookup: HashMap::new(),
            strings: HashMap::new(),
            runtime_strings: HashMap::new(),
            // Start past the null word (0..4) and the first block's 12-byte header (4..16).
            next_string_offset: 4 + HEAP_HEADER_SIZE,
            loop_counter: 0,
            loop_stack: Vec::new(),
            pending_loop_label: None,
            current_generic_bindings: HashMap::new(),
            current_mangled_name: None,
            function_indices: HashMap::new(),
            alloc_depth: 0,
            tmp_depth: 0,
        }
    }
}

/// Generates WebAssembly (WAT) code from the given syntax tree and semantic info.
#[allow(dead_code)]
pub struct WasmGenerator<'a> {
    pub syntax_tree: &'a SyntaxTree<'a>,
    pub symbol_map: &'a HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
    pub struct_table: &'a crate::semantics::struct_table::StructTable,
    pub instantiated_generics: &'a HashMap<String, (crate::semantics::analyzer::GenericBindings, &'a crate::syntax::nodes::FunctionNode<'a>)>,
    pub struct_methods: &'a Vec<(&'a crate::syntax::nodes::FunctionNode<'a>, crate::semantics::analyzer::GenericBindings)>,
    /// Registered enums: name -> (member -> i32 value). Enum members lower to `i32.const`.
    pub enums: &'a crate::semantics::analyzer::EnumTable,
    /// Mutable working state accumulated during emission.
    pub ctx: CodegenContext,
}

impl<'a> CodeGenerator<'a> for WasmGenerator<'a> {
    fn generate(&mut self) -> Result<String, Error> {
        let indented = self.build()?;
        Ok(indented.to_string())
    }
}

impl<'a> WasmGenerator<'a> {
    /// Creates a new instance of WasmGenerator
    pub fn new(syntax_tree: &'a SyntaxTree<'a>, semantic_info: &'a SemanticInfo) -> Self {
        Self {
            syntax_tree,
            symbol_map: &semantic_info.hash_map,
            function_table: &semantic_info.function_table,
            struct_table: &semantic_info.struct_table,
            instantiated_generics: &semantic_info.instantiated_generics,
            struct_methods: &semantic_info.struct_methods,
            enums: &semantic_info.enums,
            ctx: CodegenContext::new(),
        }
    }

    /// Number of `$ctor_base{n}` scratch locals declared per function. Bounds the supported
    /// nesting depth of literal heap constructors; deeper nesting falls back to the last slot.
    pub const CTOR_BASE_POOL: usize = 16;

    /// Number of `$tmp{n}` locals declared per function for releasing owned-reference call
    /// arguments after a call. Bounds the count of simultaneously-live owned argument temporaries
    /// across nested calls; deeper nesting falls back to the last slot.
    pub const TMP_POOL: usize = 16;

    /// Returns the name of the base-pointer local for the current constructor nesting depth,
    /// clamped to the declared pool so it always refers to a real local.
    pub fn ctor_base_local(&self) -> String {
        let idx = self.ctx.alloc_depth.min(Self::CTOR_BASE_POOL - 1);
        format!("$ctor_base{}", idx)
    }
}
