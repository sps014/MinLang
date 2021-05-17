use std::{str::Chars, usize};

use super::syntax_kind::*;
use super::syntax_token::*;
pub struct Lexer {
    input_text: String,
    current: i32,
}
impl Lexer {
    pub fn new(input_text: &str) -> Lexer {
        Lexer {
            input_text: String::from(input_text),
            current: 0,
        }
    }
    pub fn input_text(&self) -> &String {
        &self.input_text
    }
    pub fn next(&mut self) {
        self.current += 1;
    }
    pub fn input_str(&self) -> &str {
        self.input_text.as_str()
    }
    pub fn current_char(&self) -> char {
        if self.current as usize >= self.input_text.len() {
            return '\0';
        }
        self.input_text.as_bytes()[self.current as usize] as char
    }
    pub fn next_token(&mut self) -> SyntaxToken {
        let mut pos = self.current as usize;
        if pos >= self.input_text.len() {
            return SyntaxToken::new(
                SyntaxKind::EndOfFileToken,
                (self.current as usize) as i32,
                "\0",
            );
        }
        let input_text = self.input_text.as_str();
        if char::is_digit(self.current_char(), 10) {
            let start = pos;
            while char::is_digit(self.current_char(), 10) {
                self.current += 1;
            }
            let length = self.current as usize - start;
            let text: &str = input_text[start..start + length].as_ref();

            return SyntaxToken::new(SyntaxKind::NumberToken, start as i32, text);
        }
        if self.current_char() == '\n' {
            self.next();
            return SyntaxToken::new(SyntaxKind::NewLineToken, self.current - 1, "\n");
        }

        if char::is_whitespace(self.current_char()) {
            let start = pos;
            while char::is_whitespace(self.current_char()) {
                self.current += 1;
            }
            let length = self.current as usize - start;
            let text: &str = input_text[start..start + length].as_ref();

            return SyntaxToken::new(SyntaxKind::WhiteSpaceToken, start as i32, text);
        }

        if self.current_char() == '+' {
            self.next();
            return SyntaxToken::new(SyntaxKind::PlusToken, self.current - 1, "+");
        } else if self.current_char() == '-' {
            self.next();
            return SyntaxToken::new(SyntaxKind::MinusToken, self.current - 1, "-");
        } else if self.current_char() == '*' {
            self.next();
            return SyntaxToken::new(SyntaxKind::StarToken, self.current - 1, "*");
        } else if self.current_char() == '/' {
            self.next();
            return SyntaxToken::new(SyntaxKind::SlashToken, self.current - 1, "/");
        } else if self.current_char() == '(' {
            self.next();
            return SyntaxToken::new(SyntaxKind::OpenParenthesisToken, self.current - 1, "(");
        } else if self.current_char() == ')' {
            self.next();
            return SyntaxToken::new(SyntaxKind::CloseParenthesisToken, self.current - 1, ")");
        }

        let text = self.current_char();
        self.next();
        SyntaxToken::new(
            SyntaxKind::BadToken,
            self.current - 1,
            text.to_string().as_str(),
        )
    }
    pub fn tokenize(&self) -> Vec<SyntaxToken> {
        vec![]
    }
}
