use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;
use std::usize;
use crate::lang::code_analysis::text::line_text::LineText;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;

///Lexes all token and all invalid tokens are reported via diagnostics
pub struct Lexer<'a> {
    input_text: String,
    current: usize,
    diagnostics: Vec<String>,
    type_regex_map:HashMap<TokenKind,&'a str>,
    line_text:Rc<LineText>
}
impl<'a> Lexer<'a> {
    //create a new instance of lexer
    pub fn new(input_text: String) -> Lexer<'a> {

        Lexer {
            input_text: input_text.clone(),
            current: 0,
            diagnostics: Vec::new(),
            type_regex_map: Lexer::create_type_regex_map(),
            line_text:Rc::new(LineText::new(input_text))
        }
    }
    fn create_type_regex_map()->HashMap<TokenKind,&'a str>
    {
        let mut map = HashMap::new();
        map.insert(TokenKind::IdentifierToken,"[a-zA-Z_][a-zA-Z0-9_]*");
        map.insert(TokenKind::NumberToken,"[0-9]+");

        map.insert(TokenKind::EqualEqualToken,r"==");
        map.insert(TokenKind::EqualToken,r"=");

        map.insert(TokenKind::SemicolonToken,r";");
        map.insert((TokenKind::ColonToken),r":");
        map.insert(TokenKind::CommaToken,r",");
        map.insert(TokenKind::DotToken,r"\.");

        map.insert(TokenKind::PlusToken,r"\+");
        map.insert(TokenKind::MinusToken,r"\-");
        map.insert(TokenKind::StarToken,r"\*");
        map.insert(TokenKind::SlashToken,r"/");

        map.insert(TokenKind::OpenParenthesisToken,r"\(");
        map.insert(TokenKind::CloseParenthesisToken,r"\)");
        map.insert(TokenKind::CurlyOpenBracketToken,r"\{");
        map.insert(TokenKind::CurlyCloseBracketToken,r"\}");

        map.insert(TokenKind::WhiteSpaceToken,r"\s+");
        return map;
    }

    //get all token
    pub fn lex_all(&mut self)->Vec<SyntaxToken>
    {
        let mut res=vec![];
        loop {
            let c=self.next_token();
            if c.kind==TokenKind::BadToken
            {
                self.diagnostics.push(c.text.clone());
            }

            else if c.kind==TokenKind::EndOfFileToken
            {
                break;
            }
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
        let current_str=self.current_str();
        for (key,value) in self.type_regex_map.iter()
        {
            let c=Lexer::do_match(&value.clone(),key.clone(),&mut self.current,&current_str,self.line_text.borrow());
            if c.is_some()
            {
                return c.unwrap();
            }
        }

        if self.current>=self.input_text.len()
        {
            return SyntaxToken::new(TokenKind::EndOfFileToken,
                                    TextSpan::new((self.current,self.current+1),self.line_text.borrow()),
                                    "\0".to_string());
        }

        let bt=self.current_char();
        self.current+=1;
        SyntaxToken::new(TokenKind::BadToken, TextSpan::new((self.current-1, self.current),self.line_text.borrow()),bt.to_string())
    }
    fn do_match(regex_str:&str,token:TokenKind,current:&mut usize,current_str:&String,line_text:&LineText)->Option<SyntaxToken>
    {
        let re=regex::Regex::new(regex_str).unwrap();
        for cap in re.captures_iter(current_str.as_str()) {
            if cap.get(0).unwrap().start()!=0
            {
                return None;
            }
            let start=*current+cap.get(0).unwrap().start();
            let end=*current+cap.get(0).unwrap().end();
            *current=end;
            return Some(SyntaxToken::new(token.clone(),TextSpan::new((start,end),line_text),cap.get(0).unwrap().as_str().to_string()));
        }
        return None;
    }
}
