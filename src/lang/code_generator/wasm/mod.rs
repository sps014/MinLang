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

/// Generates WebAssembly (WAT) code from the given syntax tree and semantic info.
#[allow(dead_code)]
pub struct WasmGenerator<'a> {
    pub syntax_tree: &'a SyntaxTree<'a>,
    pub symbol_map: &'a HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
    // key 1: function name, key 2: parameter name
    pub combined_symbol_lookup: HashMap<String, HashMap<String, Type>>,
    pub strings: HashMap<String, usize>,
    pub next_string_offset: usize,
    pub loop_counter: usize,
    pub loop_stack: Vec<usize>,
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
            combined_symbol_lookup: HashMap::new(),
            strings: HashMap::new(),
            next_string_offset: 0,
            loop_counter: 0,
            loop_stack: Vec::new(),
        }
    }
}
