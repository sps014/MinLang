use std::rc::Rc;
use crate::syntax::token::syntax_token::SyntaxToken;

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
    pub methods: Vec<crate::syntax::nodes::function::FunctionNode<'a>>,
    pub is_exported: bool,
    /// Source file this declaration came from; set during multi-file merge so semantic
    /// diagnostics can report the correct file. `None` for synthesized nodes.
    pub file_path: Option<Rc<str>>,
}

impl<'a> StructDeclarationNode<'a> {
    pub fn new(name: SyntaxToken, generic_parameters: Option<Vec<SyntaxToken>>, fields: Vec<StructFieldNode>, methods: Vec<crate::syntax::nodes::function::FunctionNode<'a>>, is_exported: bool) -> Self {
        Self { name, generic_parameters, fields, methods, is_exported, file_path: None }
    }
}
