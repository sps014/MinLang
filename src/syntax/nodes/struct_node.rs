use crate::syntax::nodes::Type;
use crate::syntax::token::syntax_token::SyntaxToken;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct StructFieldNode {
    pub attributes: Vec<crate::syntax::nodes::AttributeNode>,
    pub name: SyntaxToken,
    /// The field type's canonical spelling as a token (carries the source position and a flat
    /// display name like `List_JsonValue`). For the structured type (which preserves generic
    /// arguments such as `List<JsonValue>`), use `field_type`.
    pub type_token: SyntaxToken,
    /// The fully parsed field type, preserving generic arguments, arrays, and nullability so
    /// generic field types (e.g. `List<JsonValue>`, `Map<string, V>`) can be instantiated and
    /// have their methods resolved.
    pub field_type: Type,
}

#[derive(Debug, Clone)]
pub struct StructDeclarationNode<'a> {
    pub attributes: Vec<crate::syntax::nodes::AttributeNode>,
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
    pub fn new(
        attributes: Vec<crate::syntax::nodes::AttributeNode>,
        name: SyntaxToken,
        generic_parameters: Option<Vec<SyntaxToken>>,
        fields: Vec<StructFieldNode>,
        methods: Vec<crate::syntax::nodes::function::FunctionNode<'a>>,
        is_exported: bool,
    ) -> Self {
        Self {
            attributes,
            name,
            generic_parameters,
            fields,
            methods,
            is_exported,
            file_path: None,
        }
    }
}
