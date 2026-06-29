use super::function::FunctionNode;
use super::struct_node::StructDeclarationNode;
use crate::syntax::token::syntax_token::SyntaxToken;
use std::rc::Rc;

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

/// Represents an `extend Type { ... }` block: a set of methods attached to an existing
/// type (a primitive, `object`, or a struct) without changing that type's runtime
/// representation. Methods are lowered exactly like struct methods (`{target}_{method}`
/// with an implicit `this` parameter).
#[derive(Debug, Clone)]
pub struct ExtendNode<'a> {
    /// The canonical name of the type being extended (e.g. `int`, `string`, `Point`).
    pub target: SyntaxToken,
    pub generic_parameters: Option<Vec<SyntaxToken>>,
    pub methods: Vec<FunctionNode<'a>>,
    /// Source file this declaration came from; set during multi-file merge so semantic
    /// diagnostics can report the correct file. `None` for synthesized nodes.
    pub file_path: Option<Rc<str>>,
}

impl<'a> ExtendNode<'a> {
    pub fn new(
        target: SyntaxToken,
        generic_parameters: Option<Vec<SyntaxToken>>,
        methods: Vec<FunctionNode<'a>>,
    ) -> ExtendNode<'a> {
        ExtendNode {
            target,
            generic_parameters,
            methods,
            file_path: None,
        }
    }
}

/// Represents the root program node in the AST
#[derive(Debug, Clone)]
pub struct ProgramNode<'a> {
    pub imports: Vec<ImportNode>,
    pub structs: Vec<StructDeclarationNode<'a>>,
    pub functions: Vec<FunctionNode<'a>>,
    pub enums: Vec<EnumDeclarationNode>,
    pub extends: Vec<ExtendNode<'a>>,
}

impl<'a> ProgramNode<'a> {
    /// Creates a new program node
    pub fn new(
        imports: Vec<ImportNode>,
        structs: Vec<StructDeclarationNode<'a>>,
        functions: Vec<FunctionNode<'a>>,
        enums: Vec<EnumDeclarationNode>,
        extends: Vec<ExtendNode<'a>>,
    ) -> ProgramNode<'a> {
        ProgramNode {
            imports,
            structs,
            functions,
            enums,
            extends,
        }
    }
}
