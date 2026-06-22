use crate::syntax::token::syntax_token::SyntaxToken;
use super::function::FunctionNode;
use super::struct_node::StructDeclarationNode;

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

/// Represents a C-style enum declaration: `enum Color { Red, Green = 5, Blue }`.
/// Members carry explicit `i32` values (auto-assigned sequentially when not specified).
#[derive(Debug, Clone)]
pub struct EnumDeclarationNode {
    pub name: SyntaxToken,
    pub members: Vec<(SyntaxToken, i32)>,
}

impl EnumDeclarationNode {
    pub fn new(name: SyntaxToken, members: Vec<(SyntaxToken, i32)>) -> EnumDeclarationNode {
        EnumDeclarationNode { name, members }
    }
}

/// Represents the root program node in the AST
#[derive(Debug, Clone)]
pub struct ProgramNode<'a> {
    pub imports: Vec<ImportNode>,
    pub structs: Vec<StructDeclarationNode<'a>>,
    pub functions: Vec<FunctionNode<'a>>,
    pub enums: Vec<EnumDeclarationNode>,
}

impl<'a> ProgramNode<'a> {
    /// Creates a new program node
    pub fn new(imports: Vec<ImportNode>, structs: Vec<StructDeclarationNode<'a>>, functions: Vec<FunctionNode<'a>>, enums: Vec<EnumDeclarationNode>) -> ProgramNode<'a> {
        ProgramNode { imports, structs, functions, enums }
    }
}
