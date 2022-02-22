use std::usize;
use regex::Regex;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;

///Lexes all token and all invalid tokens are reported via diagnostics
pub struct Lexer {
    input_text: String,
    current: usize,
    diagnostics: Vec<String>,
}
impl Lexer {
    //create a new instance of lexer
    pub fn new(input_text: String) -> Lexer {
        Lexer {
            input_text,
            current: 0,
            diagnostics: Vec::new(),
        }
    }

    //get all token
    pub fn lex_all(&mut self)->Vec<SyntaxToken>
    {
        let mut res=vec![];
        loop {
            let c=self.next_token();
            if c.kind==TokenKind::EndOfFileToken
            {
                break;
            }
            println!("{:?}",c);
            res.push(c);
        }
        res
    }

    /// increment to next token
    fn next(&mut self) {
        self.current += 1;
    }

    //return current character if it is not in index range then returns end of file character
    fn current_char(&self) -> char {
        if self.current >= self.input_text.len() {
            return '\0';
        }
        self.input_text.as_bytes()[self.current] as char
    }

    //returns the current token if it is valid otherwise returns an eof token
    fn next_token(&mut self) -> SyntaxToken
    {
        let mut c_m=self.do_match("==", TokenKind::EqualEqualToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match("=", TokenKind::EqualToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(";", TokenKind::SemicolonToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(":", TokenKind::ColonToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match("\\(", TokenKind::OpenParenthesisToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match("\\)", TokenKind::CloseParenthesisToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match("\\{", TokenKind::CurlyOpenBracketToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match("\\}", TokenKind::CurlyCloseBracketToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(",", TokenKind::CommaToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match("\\+", TokenKind::DotToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }

        let bt=self.current_char();
        self.current+=1;
        SyntaxToken::new(TokenKind::BadToken, TextSpan::new((self.current-1, self.current)),bt.to_string())
    }
    pub fn match_tokens(&mut self)->Option<SyntaxToken>
    {
        let token = self.next_token();
        if token.kind == TokenKind::EndOfFileToken {
            return None;
        }
        Some(token)
    }
    fn do_match(&mut self,regex_str:&str,token:TokenKind)->Option<SyntaxToken>
    {
        let re=regex::Regex::new(regex_str).unwrap();
        let slice=&self.input_text[self.current..];
        let match_pos=re.find(slice);
        if match_pos.is_some()
        {
            let res=match_pos.unwrap();
            if res.start()!=0
            {
                return None;
            }
            let cp_start=self.current;
            let cp_end=self.current+res.end()+1;
            let cp_str=&self.input_text[cp_start..=cp_end];
            self.current=cp_end;

            return Some(

                SyntaxToken::new(token,
                                 TextSpan::new((cp_start,cp_end)),
                                 cp_str.to_string())
            );
        }
        return None;
    }
}
