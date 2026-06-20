use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::types::Type;

/// Represents an expression node in the AST
#[derive(Debug, Clone)]
pub enum ExpressionNode<'a> {
    Literal(Type),
    Binary(&'a ExpressionNode<'a>, SyntaxToken, &'a ExpressionNode<'a>),
    Unary(SyntaxToken, &'a ExpressionNode<'a>),
    Identifier(SyntaxToken),
    Parenthesized(&'a ExpressionNode<'a>),
    FunctionCall(SyntaxToken, Vec<ExpressionNode<'a>>),
}
