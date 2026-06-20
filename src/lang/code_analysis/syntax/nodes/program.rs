use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::function::FunctionNode;

/// Represents an import declaration in the AST
#[derive(Debug, Clone)]
pub struct ImportNode {
    pub module_name: SyntaxToken,
}

impl ImportNode {
    /// Creates a new import node
    pub fn new(module_name: SyntaxToken) -> ImportNode {
        ImportNode { module_name }
    }
}

/// Represents the root program node in the AST
#[derive(Debug, Clone)]
pub struct ProgramNode<'a> {
    pub imports: Vec<ImportNode>,
    pub functions: Vec<FunctionNode<'a>>,
}

impl<'a> ProgramNode<'a> {
    /// Creates a new program node
    pub fn new(imports: Vec<ImportNode>, functions: Vec<FunctionNode<'a>>) -> ProgramNode<'a> {
        ProgramNode { imports, functions }
    }
}
