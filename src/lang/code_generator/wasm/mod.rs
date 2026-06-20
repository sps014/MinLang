use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::io::Error;

use crate::lang::code_analysis::syntax::nodes::Type;
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::semantic_analysis::analyzer::SemanticInfo;
use crate::lang::semantic_analysis::function_table::FunctionTable;
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use crate::lang::code_generator::CodeGenerator;

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

/// Generates WebAssembly (WAT) code from the given syntax tree and semantic info.
#[allow(dead_code)]
pub struct WasmGenerator<'a> {
    pub syntax_tree: &'a SyntaxTree<'a>,
    pub symbol_map: &'a HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
    pub struct_table: &'a crate::lang::semantic_analysis::struct_table::StructTable,
    // key 1: function name, key 2: parameter name
    pub combined_symbol_lookup: HashMap<String, HashMap<String, Type>>,
    pub strings: HashMap<String, usize>,
    /// Runtime-only string literals (e.g. "true", "null", struct labels) interned by the object
    /// protocol; maps raw (already unquoted) content -> data-segment offset.
    pub runtime_strings: HashMap<String, usize>,
    pub next_string_offset: usize,
    pub loop_counter: usize,
    pub loop_stack: Vec<usize>,
    /// Active generic parameter -> concrete type bindings while emitting a monomorphized
    /// generic function body (empty when not inside one).
    pub current_generic_bindings: HashMap<String, String>,
    pub current_mangled_name: Option<String>,
    pub instantiated_generics: &'a HashMap<String, (crate::lang::semantic_analysis::analyzer::GenericBindings, &'a crate::lang::code_analysis::syntax::nodes::FunctionNode<'a>)>,
    pub struct_methods: &'a Vec<(&'a crate::lang::code_analysis::syntax::nodes::FunctionNode<'a>, crate::lang::semantic_analysis::analyzer::GenericBindings)>,
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
            combined_symbol_lookup: HashMap::new(),
            strings: HashMap::new(),
            runtime_strings: HashMap::new(),
            // Start past the null word (0..4) and the first block's 12-byte header (4..16).
            next_string_offset: 4 + HEAP_HEADER_SIZE,
            loop_counter: 0,
            loop_stack: Vec::new(),
            current_generic_bindings: HashMap::new(),
            current_mangled_name: None,
            instantiated_generics: &semantic_info.instantiated_generics,
            struct_methods: &semantic_info.struct_methods,
        }
    }
}
