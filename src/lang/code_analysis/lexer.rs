use std::usize;

use super::syntax_kind::*;
use super::syntax_token::*;

pub struct Lexer {
    input_text: String,
    current: usize,
    diagnostics: Vec<String>,
}
impl Lexer {
    pub fn new(input_text: &str) -> Lexer {
        Lexer {
            input_text: String::from(input_text),
            current: 0,
            diagnostics: Vec::new(),
        }
    }
    pub fn next(&mut self) {
        self.current += 1;
    }
    pub fn current_char(&self) -> char {
        if self.current >= self.input_text.len() {
            return '\0';
        }
        self.input_text.as_bytes()[self.current as usize] as char
    }
    pub fn next_token(&mut self) -> SyntaxToken {
        let pos = self.current;
        if pos >= self.input_text.len() {
            return SyntaxToken::new(SyntaxKind::EndOfFileToken, pos, "\0");
        }
        let input_text = self.input_text.as_str();
        if char::is_digit(self.current_char(), 10) {
            while char::is_digit(self.current_char(), 10) {
                self.current += 1;
            }
            let length = self.current - pos;
            let text: &str = input_text[pos..pos + length].as_ref();

            return SyntaxToken::new(SyntaxKind::NumberToken, pos, text);
        }
        if self.current_char() == '\n' {
            self.next();
            return SyntaxToken::new(SyntaxKind::NewLineToken, pos, "\n");
        }
        if self.current_char() == '_' || char::is_alphabetic(self.current_char()) {
            let start = self.current;
            self.current += 1;
            while self.current_char() == '_'
                || char::is_alphabetic(self.current_char())
                || char::is_digit(self.current_char(), 10)
            {
                self.current += 1;
            }
            let length = self.current - pos;
            let text: &str = input_text[pos..pos + length].as_ref();

            return SyntaxToken::new(SyntaxKind::IdentifierToken, pos, text);
        }

        if char::is_whitespace(self.current_char()) {
            while char::is_whitespace(self.current_char()) {
                self.current += 1;
            }
            let length = self.current - pos;
            let text: &str = input_text[pos..pos + length].as_ref();

            return SyntaxToken::new(SyntaxKind::WhiteSpaceToken, pos, text);
        }

        if self.current_char() == '+' {
            self.next();
            return SyntaxToken::new(SyntaxKind::PlusToken, pos, "+");
        } else if self.current_char() == '-' {
            self.next();
            return SyntaxToken::new(SyntaxKind::MinusToken, pos, "-");
        } else if self.current_char() == '*' {
            self.next();
            return SyntaxToken::new(SyntaxKind::StarToken, pos, "*");
        } else if self.current_char() == '/' {
            self.next();
            return SyntaxToken::new(SyntaxKind::SlashToken, pos, "/");
        } else if self.current_char() == '(' {
            self.next();
            return SyntaxToken::new(SyntaxKind::OpenParenthesisToken, pos, "(");
        } else if self.current_char() == ')' {
            self.next();
            return SyntaxToken::new(SyntaxKind::CloseParenthesisToken, pos, ")");
        } else if self.current_char() == '&' {
            self.next();
            return SyntaxToken::new(SyntaxKind::BitWiseAmpersandToken, pos, "&");
        } else if self.current_char() == '|' {
            self.next();
            return SyntaxToken::new(SyntaxKind::BitWisePipeToken, pos, "|");
        } 
        else if self.current_char()=='='
        {
            self.next();
            if self.current_char()=='='
            {
                self.next();
                return SyntaxToken::new(SyntaxKind::EqualEqualToken, pos, "==");

            }
            else
            {
                return SyntaxToken::new(SyntaxKind::EqualToken, pos, "=");
            }
        }
        
        else if self.current_char()=='>'
        {
            self.next();
            if self.current_char()=='='
            {
                self.next();
                return SyntaxToken::new(SyntaxKind::GreaterThanEqualToken, pos, ">=");

            }
            else
            {
                return SyntaxToken::new(SyntaxKind::GreaterThanToken, pos, ">");
            }
        }
        else if self.current_char()=='<'
        {
            self.next();
            if self.current_char()=='='
            {
                self.next();
                return SyntaxToken::new(SyntaxKind::SmallerThanEqualToken, pos, "<=");

            }
            else
            {
                return SyntaxToken::new(SyntaxKind::SmallerThanToken, pos, "<");
            }
        }

        let text = self.current_char();
        self.next();
        self.diagnostics
            .push(format!("Unexpected token {} at position {}", text, pos));
        SyntaxToken::new(SyntaxKind::BadToken, pos, text.to_string().as_str())
    }
}
