use crate::lang::code_analysis::text::text_span::TextSpan;
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

impl<'a> ExpressionNode<'a> {
    /// Returns a representative source span for this expression, derived from an existing
    /// token in the node (no positions are stored separately). Used to attach line/column
    /// information to semantic diagnostics. Returns `None` only when nothing positional is
    /// available (e.g. an empty array literal or the `null` literal).
    pub fn position(&self) -> Option<TextSpan> {
        match self {
            ExpressionNode::Literal(t) => t.get_span(),
            ExpressionNode::Identifier(token)
            | ExpressionNode::FunctionCall(token, _, _)
            | ExpressionNode::StructInstantiation(token, _, _)
            | ExpressionNode::MemberAccess(_, token)
            | ExpressionNode::MethodCall(_, token, _, _)
            | ExpressionNode::Binary(_, token, _)
            | ExpressionNode::Unary(token, _) => Some(token.position.clone()),
            ExpressionNode::Parenthesized(inner)
            | ExpressionNode::IsExpression(inner, _) => inner.position(),
            ExpressionNode::IndexAccess(array_expr, _) => array_expr.position(),
            ExpressionNode::Cast(target_type, expr) => target_type.get_span().or_else(|| expr.position()),
            ExpressionNode::ArrayLiteral(elements) => elements.first().and_then(|e| e.position()),
        }
    }
}
