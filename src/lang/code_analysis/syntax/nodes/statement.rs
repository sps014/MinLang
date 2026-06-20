use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::expression::ExpressionNode;

/// Represents a statement node in the AST
#[derive(Debug, Clone)]
pub enum StatementNode<'a> {
    Assignment(SyntaxToken, ExpressionNode<'a>),
    Declaration(SyntaxToken, ExpressionNode<'a>),
    FunctionInvocation(SyntaxToken, Vec<ExpressionNode<'a>>),
    Return(Option<ExpressionNode<'a>>),
    /// If condition, then body, else if pairs, else body
    IfElse(
        ExpressionNode<'a>,
        &'a [StatementNode<'a>],
        Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>,
        Option<&'a [StatementNode<'a>]>,
    ),
    While(ExpressionNode<'a>, &'a [StatementNode<'a>]),
    For(
        Option<&'a StatementNode<'a>>,
        Option<ExpressionNode<'a>>,
        Option<&'a StatementNode<'a>>,
        &'a [StatementNode<'a>],
    ),
    Break,
    Continue,
}
