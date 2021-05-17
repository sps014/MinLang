use super::syntax_kind::*;
#[derive(Debug)]
pub struct SyntaxToken {
    pub kind: SyntaxKind,
    position: i32,
    text: String,
}
impl SyntaxToken {
    pub fn new(kind: SyntaxKind, pos: i32, text: &str) -> SyntaxToken {
        SyntaxToken {
            kind: kind,
            position: pos,
            text: String::from(text),
        }
    }
}
