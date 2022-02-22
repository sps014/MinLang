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
        if self.current < self.input_text.len() {
            self.input_text.chars().nth(self.current).unwrap()
        } else {
            '\0'
        }
    }
    //returns string sliced from currentor returns EOF
    fn current_str(&self) -> String {
        if self.current >= self.input_text.len() {
            return "\0".to_string();
        }
        self.input_text[self.current..].to_string()
    }

    //returns the current token if it is valid otherwise returns an eof token
    fn next_token(&mut self) -> SyntaxToken
    {
        let mut c_m=self.do_match("==", TokenKind::EqualEqualToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"=", TokenKind::EqualToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r";", TokenKind::SemicolonToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r":", TokenKind::ColonToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"\(", TokenKind::OpenParenthesisToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"\)", TokenKind::CloseParenthesisToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"\{", TokenKind::CurlyOpenBracketToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"\}", TokenKind::CurlyCloseBracketToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r",", TokenKind::CommaToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"\+", TokenKind::DotToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }
        c_m=self.do_match(r"\s+", TokenKind::WhiteSpaceToken);
        if c_m.is_some()
        {
            return c_m.unwrap();
        }


        else if self.current>=self.input_text.len()
        {
            return SyntaxToken::new(TokenKind::EndOfFileToken,
                                    TextSpan::new((self.current,self.current+1)),
                                    "\0".to_string());
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
        let slice=self.current_str();
        for cap in re.captures_iter(slice.as_str()) {
            if cap.get(0).unwrap().start()!=0
            {
                return None;
            }
            let start=self.current+cap.get(0).unwrap().start();
            let end=self.current+cap.get(0).unwrap().end();
            self.current=end;
            return Some(SyntaxToken::new(token,TextSpan::new((start,end)),cap.get(0).unwrap().as_str().to_string()));
        }
        return None;
    }
}
