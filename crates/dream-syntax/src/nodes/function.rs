use super::statement::StatementNode;
use super::types::Type;
use crate::token::syntax_token::SyntaxToken;
use std::rc::Rc;

/// Represents a function parameter in the AST
#[derive(Debug, Clone)]
pub struct ParameterNode {
    pub name: SyntaxToken,
    pub type_: Type,
    /// An optional default value, restricted to a constant literal (`= 5`, `= "hi"`, `= true`,
    /// `= -1`, `= null`). When present, the parameter may be omitted at a call site and the default
    /// is substituted. `None` for required parameters and all synthesized parameters (e.g. `this`).
    pub default: Option<Type>,
}

impl ParameterNode {
    /// Creates a new required parameter node (no default value).
    pub fn new(name: SyntaxToken, type_: Type) -> ParameterNode {
        ParameterNode {
            name,
            type_,
            default: None,
        }
    }

    /// Creates a parameter node with a constant-literal default value.
    pub fn with_default(name: SyntaxToken, type_: Type, default: Option<Type>) -> ParameterNode {
        ParameterNode {
            name,
            type_,
            default,
        }
    }
}

/// Represents a function declaration in the AST
#[derive(Debug, Clone)]
pub struct FunctionNode<'a> {
    pub attributes: Vec<crate::nodes::AttributeNode>,
    pub name: SyntaxToken,
    pub generic_parameters: Option<Vec<SyntaxToken>>,
    pub return_type: Option<Type>,
    pub parameters: Vec<ParameterNode>,
    pub body: &'a [StatementNode<'a>],
    /// True when the declaration is marked `public`: it is visible to other modules and (for
    /// top-level functions) emitted as a WebAssembly export. Private (the default) symbols are
    /// module-internal.
    pub is_public: bool,
    /// True for `extern fun` declarations: the function has no body and is lowered to a WASM
    /// import instead of a defined function. Used for JS interop.
    pub is_extern: bool,
    /// True for `static fun` methods declared inside a `struct`/`extend` block: the method has no
    /// implicit `this` parameter and is dispatched via `Type.method(...)` instead of `value.method(...)`.
    pub is_static: bool,
    /// True for `async fun` declarations: calling the function eagerly starts a task and yields a
    /// `Future<T>` handle. The body is lowered to a resumable state machine driven by the scheduler.
    pub is_async: bool,
    /// Source file this declaration came from; set during multi-file merge so semantic
    /// diagnostics can report the correct file. `None` for synthesized nodes.
    pub file_path: Option<Rc<str>>,
}

impl<'a> FunctionNode<'a> {
    /// Creates a new function node
    pub fn new(
        attributes: Vec<crate::nodes::AttributeNode>,
        name: SyntaxToken,
        generic_parameters: Option<Vec<SyntaxToken>>,
        return_type: Option<Type>,
        parameters: Vec<ParameterNode>,
        body: &'a [StatementNode<'a>],
        is_public: bool,
    ) -> FunctionNode<'a> {
        FunctionNode {
            attributes,
            name,
            generic_parameters,
            return_type,
            parameters,
            body,
            is_public,
            is_extern: false,
            is_static: false,
            is_async: false,
            file_path: None,
        }
    }
}
