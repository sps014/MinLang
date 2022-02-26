use crate::lang::code_analysis::text::text_span::TextSpan;
use super::token_kind::*;

///Represents a basic token in any given language
#[derive(Debug,Clone)]
pub struct SyntaxToken {
    pub kind: TokenKind,
    pub position:TextSpan,
    pub text: String,
}
impl SyntaxToken
{
    ///create new instance of syntax token from  the type of token and position and text in the token
    pub fn new(kind: TokenKind, pos: TextSpan, text: String) -> SyntaxToken {
        SyntaxToken {
            kind,
            position: pos,
            text
        }
    }
    ///returns a trimmed text value of the token
    #[allow(dead_code)]
    pub fn get_trim(&self)->String
    {
        self.text.trim().to_string()
    }
}

impl PartialEq for SyntaxToken {
    fn eq(&self, other: &SyntaxToken) -> bool {
        self.kind == other.kind && self.text == other.text && self.position.start== other.position.start
    }
}