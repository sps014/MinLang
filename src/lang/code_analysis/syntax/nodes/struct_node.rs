use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug, Clone)]
pub struct StructFieldNode {
    pub name: SyntaxToken,
    pub type_token: SyntaxToken,
}

#[derive(Debug, Clone)]
pub struct StructDeclarationNode {
    pub name: SyntaxToken,
    pub fields: Vec<StructFieldNode>,
}

impl StructDeclarationNode {
    pub fn new(name: SyntaxToken, fields: Vec<StructFieldNode>) -> Self {
        Self { name, fields }
    }
}
