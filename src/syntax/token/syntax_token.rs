use crate::syntax::text::text_span::TextSpan;
use super::token_kind::*;
use super::syntax_trivia::SyntaxTrivia;

///Represents a basic token in any given language
#[derive(Debug,Clone)]
pub struct SyntaxToken {
    pub kind: TokenKind,
    pub position:TextSpan,
    pub text: String,
    pub leading_trivia: Vec<SyntaxTrivia>,
    pub trailing_trivia: Vec<SyntaxTrivia>,
}
impl SyntaxToken
{
    ///create new instance of syntax token from  the type of token and position and text in the token
    pub fn new(kind: TokenKind, pos: TextSpan, text: String) -> SyntaxToken {
        SyntaxToken {
            kind,
            position: pos,
            text,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        }
    }

    pub fn with_trivia(mut self, leading: Vec<SyntaxTrivia>, trailing: Vec<SyntaxTrivia>) -> Self {
        self.leading_trivia = leading;
        self.trailing_trivia = trailing;
        self
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