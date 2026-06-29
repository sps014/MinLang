use crate::syntax::text::text_span::TextSpan;
use super::token_kind::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxTrivia {
    pub kind: TokenKind,
    pub position: TextSpan,
    pub text: String,
}

impl SyntaxTrivia {
    pub fn new(kind: TokenKind, position: TextSpan, text: String) -> Self {
        Self { kind, position, text }
    }
}
