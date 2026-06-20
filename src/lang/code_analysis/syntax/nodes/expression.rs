use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::types::Type;

/// Represents an expression node in the AST
#[derive(Debug, Clone)]
pub enum ExpressionNode<'a> {
    Literal(Type),
    ArrayLiteral(Vec<ExpressionNode<'a>>),
    Binary(&'a ExpressionNode<'a>, SyntaxToken, &'a ExpressionNode<'a>),
    Unary(SyntaxToken, &'a ExpressionNode<'a>),
    Identifier(SyntaxToken),
    Parenthesized(&'a ExpressionNode<'a>),
    FunctionCall(SyntaxToken, Option<Vec<Type>>, Vec<ExpressionNode<'a>>),
    IndexAccess(&'a ExpressionNode<'a>, &'a ExpressionNode<'a>),
    Cast(Type, &'a ExpressionNode<'a>),
    StructInstantiation(SyntaxToken, Option<Vec<Type>>, Vec<(SyntaxToken, ExpressionNode<'a>)>),
    MemberAccess(&'a ExpressionNode<'a>, SyntaxToken),
    IsExpression(&'a ExpressionNode<'a>, Type),
    MethodCall(&'a ExpressionNode<'a>, SyntaxToken, Option<Vec<Type>>, Vec<ExpressionNode<'a>>),
}
