use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::statement::StatementNode;
use super::types::Type;

/// Represents a function parameter in the AST
#[derive(Debug, Clone)]
pub struct ParameterNode {
    pub name: SyntaxToken,
    pub type_: Type,
}

impl ParameterNode {
    /// Creates a new parameter node
    pub fn new(name: SyntaxToken, type_: Type) -> ParameterNode {
        ParameterNode { name, type_ }
    }
}

/// Represents a function declaration in the AST
#[derive(Debug, Clone)]
pub struct FunctionNode<'a> {
    pub name: SyntaxToken,
    pub generic_parameters: Option<Vec<SyntaxToken>>,
    pub return_type: Option<Type>,
    pub parameters: Vec<ParameterNode>,
    pub body: &'a [StatementNode<'a>],
    pub is_exported: bool,
}

impl<'a> FunctionNode<'a> {
    /// Creates a new function node
    pub fn new(
        name: SyntaxToken,
        generic_parameters: Option<Vec<SyntaxToken>>,
        return_type: Option<Type>,
        parameters: Vec<ParameterNode>,
        body: &'a [StatementNode<'a>],
        is_exported: bool,
    ) -> FunctionNode<'a> {
        FunctionNode {
            name,
            generic_parameters,
            return_type,
            parameters,
            body,
            is_exported,
        }
    }
}
