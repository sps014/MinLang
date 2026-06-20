use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug, Clone)]
pub struct StructFieldNode {
    pub name: SyntaxToken,
    pub type_token: SyntaxToken,
}

#[derive(Debug, Clone)]
pub struct StructDeclarationNode {
    pub name: SyntaxToken,
    pub generic_parameters: Option<Vec<SyntaxToken>>,
    pub fields: Vec<StructFieldNode>,
    pub is_exported: bool,
}

impl StructDeclarationNode {
    pub fn new(name: SyntaxToken, generic_parameters: Option<Vec<SyntaxToken>>, fields: Vec<StructFieldNode>, is_exported: bool) -> Self {
        Self { name, generic_parameters, fields, is_exported }
    }
}
