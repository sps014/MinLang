use std::rc::Rc;
use crate::syntax::token::syntax_token::SyntaxToken;
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
    /// True when the declaration carried the `@override` attribute. Used to mark object-protocol
    /// method overrides (`to_string`, `hash_code`) on structs.
    pub is_override: bool,
    /// True for `extern fun` declarations: the function has no body and is lowered to a WASM
    /// import instead of a defined function. Used for JS interop.
    pub is_extern: bool,
    /// True for `static fun` methods declared inside a `struct`/`extend` block: the method has no
    /// implicit `this` parameter and is dispatched via `Type.method(...)` instead of `value.method(...)`.
    pub is_static: bool,
    /// True for `async fun` declarations: calling the function eagerly starts a task and yields a
    /// `Future<T>` handle. The body is lowered to a resumable state machine driven by the scheduler.
    pub is_async: bool,
    /// WASM import module for an `extern` function. Defaults to `"env"` when unspecified.
    pub import_module: Option<String>,
    /// WASM import field name for an `extern` function. Defaults to the function name.
    pub import_name: Option<String>,
    /// Source file this declaration came from; set during multi-file merge so semantic
    /// diagnostics can report the correct file. `None` for synthesized nodes.
    pub file_path: Option<Rc<str>>,
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
            is_override: false,
            is_extern: false,
            is_static: false,
            is_async: false,
            import_module: None,
            import_name: None,
            file_path: None,
        }
    }
}
