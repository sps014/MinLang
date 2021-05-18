use std::any::Any;

use crate::lang::code_analysis::syntax_kind::SyntaxKind;

use super::syntax_token::SyntaxToken;

pub enum SyntaxNode {
    NumberExpressionSyntax(SyntaxToken),
    BinaryExpressionSyntax(Box<SyntaxNode>, SyntaxToken, Box<SyntaxNode>),
    ParenthesizedExpressionSyntax(SyntaxToken, Box<SyntaxNode>, SyntaxToken),
}
