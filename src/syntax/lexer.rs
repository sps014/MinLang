use std::rc::Rc;
use logos::Logos;
use crate::syntax::text::line_text::LineText;
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::driver::diagnostics::DiagnosticBag;

///Lex's all token and all invalid tokens are reported via diagnostics
pub struct Lexer {
    input_text: String,
    line_text: Rc<LineText>,
}

impl Lexer {
    //create a new instance of lexer
    pub fn new(input_text: String) -> Lexer {
        Lexer {
            line_text: Rc::new(LineText::new(input_text.clone())),
            input_text,
        }
    }

    //get all token
    pub fn lex_all(&mut self, diagnostics: &mut DiagnosticBag) -> Vec<SyntaxToken> {
        let mut res: Vec<SyntaxToken> = vec![];
        let mut lexer = TokenKind::lexer(&self.input_text);
        
        let mut pending_leading_trivia = Vec::new();
        let mut last_token_line = 0;

        while let Some(kind) = lexer.next() {
            let span = lexer.span();
            let text = lexer.slice().to_string();
            
            let kind = kind.unwrap_or(TokenKind::BadToken);

            if kind == TokenKind::BadToken {
                let text_span = TextSpan::new((span.start, span.end), &self.line_text);
                diagnostics.report_error(format!("unexpected token '{}'", text), Some(text_span));
                continue;
            } else if kind == TokenKind::WhiteSpaceToken {
                continue;
            } else if kind == TokenKind::LineCommentToken || kind == TokenKind::BlockCommentToken {
                let text_span = TextSpan::new((span.start, span.end), &self.line_text);
                let trivia = crate::syntax::token::syntax_trivia::SyntaxTrivia::new(kind, text_span, text);
                let comment_line = self.line_text.get_point(span.start).0;
                
                if !res.is_empty() && comment_line == last_token_line && pending_leading_trivia.is_empty() {
                    res.last_mut().unwrap().trailing_trivia.push(trivia);
                } else {
                    pending_leading_trivia.push(trivia);
                }
                continue;
            }

            last_token_line = self.line_text.get_point(span.end).0;
            let mut token = SyntaxToken::new(
                kind,
                TextSpan::new((span.start, span.end), &self.line_text),
                text,
            );
            token.leading_trivia = std::mem::take(&mut pending_leading_trivia);
            res.push(token);
        }
        
        let mut eof_token = SyntaxToken::new(
            TokenKind::EndOfFileToken,
            TextSpan::new((self.input_text.len(), self.input_text.len() + 1), &self.line_text),
            "\0".to_string(),
        );
        eof_token.leading_trivia = pending_leading_trivia;
        res.push(eof_token);
        
        res
    }
}

#[cfg(test)]
#[path = "tests/lexer_tests.rs"]
mod tests;
