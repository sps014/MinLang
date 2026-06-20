use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use super::expression::ExpressionNode;
use super::types::Type;

/// Represents a statement node in the AST
#[derive(Debug, Clone)]
pub enum StatementNode<'a> {
    Assignment(SyntaxToken, ExpressionNode<'a>),
    IndexAssignment(&'a ExpressionNode<'a>, &'a ExpressionNode<'a>, ExpressionNode<'a>),
    MemberAssignment(&'a ExpressionNode<'a>, SyntaxToken, ExpressionNode<'a>),
    Declaration(SyntaxToken, Option<Type>, ExpressionNode<'a>),
    FunctionInvocation(SyntaxToken, Option<Vec<Type>>, Vec<ExpressionNode<'a>>),
    MethodInvocation(&'a ExpressionNode<'a>, SyntaxToken, Option<Vec<Type>>, Vec<ExpressionNode<'a>>),
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
