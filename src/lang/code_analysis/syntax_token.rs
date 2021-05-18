use super::syntax_kind::*;
#[derive(Debug,Clone)]
pub struct SyntaxToken {
    pub kind: SyntaxKind,
    pub position: usize,
    pub text: String,
}
impl SyntaxToken {
    pub fn new(kind: SyntaxKind, pos: usize, text: &str) -> SyntaxToken {
        SyntaxToken {
            kind,
            position: pos,
            text: String::from(text),
        }
    }
}
