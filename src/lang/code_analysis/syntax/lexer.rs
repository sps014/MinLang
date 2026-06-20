use std::rc::Rc;
use logos::Logos;
use crate::lang::code_analysis::text::line_text::LineText;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;

///Lex's all token and all invalid tokens are reported via diagnostics
pub struct Lexer {
    input_text: String,
    diagnostics: Vec<String>,
    line_text: Rc<LineText>,
}

impl Lexer {
    //create a new instance of lexer
    pub fn new(input_text: String) -> Lexer {
        Lexer {
            line_text: Rc::new(LineText::new(input_text.clone())),
            input_text,
            diagnostics: Vec::new(),
        }
    }

    //get all token
    pub fn lex_all(&mut self) -> Vec<SyntaxToken> {
        self.diagnostics.clear();
        let mut res = vec![];
        let mut lexer = TokenKind::lexer(&self.input_text);

        while let Some(kind) = lexer.next() {
            let span = lexer.span();
            let text = lexer.slice().to_string();
            
            let kind = kind.unwrap_or(TokenKind::BadToken);

            if kind == TokenKind::BadToken {
                let text_span = TextSpan::new((span.start, span.end), &self.line_text);
                self.diagnostics.push(format!("unexpected token '{}' at {}", text, text_span.get_point_str()));
                continue;
            } else if kind == TokenKind::WhiteSpaceToken ||
                      kind == TokenKind::LineCommentToken ||
                      kind == TokenKind::BlockCommentToken {
                continue;
            }

            res.push(SyntaxToken::new(
                kind,
                TextSpan::new((span.start, span.end), &self.line_text),
                text,
            ));
        }
        
        res.push(SyntaxToken::new(
            TokenKind::EndOfFileToken,
            TextSpan::new((self.input_text.len(), self.input_text.len() + 1), &self.line_text),
            "\0".to_string(),
        ));
        
        res
    }
}
