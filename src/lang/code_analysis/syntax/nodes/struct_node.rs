use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug, Clone)]
pub struct StructFieldNode {
    pub name: SyntaxToken,
    pub type_token: SyntaxToken,
}

#[derive(Debug, Clone)]
pub struct StructDeclarationNode<'a> {
    pub name: SyntaxToken,
    pub generic_parameters: Option<Vec<SyntaxToken>>,
    pub fields: Vec<StructFieldNode>,
    pub methods: Vec<crate::lang::code_analysis::syntax::nodes::function::FunctionNode<'a>>,
    pub is_exported: bool,
}

impl<'a> StructDeclarationNode<'a> {
    pub fn new(name: SyntaxToken, generic_parameters: Option<Vec<SyntaxToken>>, fields: Vec<StructFieldNode>, methods: Vec<crate::lang::code_analysis::syntax::nodes::function::FunctionNode<'a>>, is_exported: bool) -> Self {
        Self { name, generic_parameters, fields, methods, is_exported }
    }
}
