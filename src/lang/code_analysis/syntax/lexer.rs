use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;
use std::usize;
use crate::lang::code_analysis::text::line_text::LineText;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;

///Lex's all token and all invalid tokens are reported via diagnostics
pub struct Lexer<'a> {
    input_text: String,
    current: usize,
    diagnostics: Vec<String>,
    type_regex_map:Vec<(TokenKind,&'a str)>,
    line_text:Rc<LineText>,
}
impl<'a> Lexer<'a> {
    //create a new instance of lexer
    pub fn new(input_text: String) -> Lexer<'a> {

        Lexer {
            input_text: input_text.clone(),
            current: 0,
            diagnostics: Vec::new(),
            type_regex_map: Lexer::create_type_regex_map(),
            line_text:Rc::new(LineText::new(input_text)),
        }
    }
    ///used to populate the type_regex_map with all the regexes on new instance of lexer
    fn create_type_regex_map()->Vec<(TokenKind,&'a str)>
    {
        let mut map = vec![];

        map.push((TokenKind::FunToken,r"fun"));
        map.push((TokenKind::IfToken,r"if"));
        map.push((TokenKind::ElseToken,r"else"));
        map.push((TokenKind::WhileToken,r"while"));

        map.push((TokenKind::DataTypeToken,r"int"));
        map.push((TokenKind::DataTypeToken,r"float"));
        map.push((TokenKind::DataTypeToken,r"void"));
        map.push((TokenKind::DataTypeToken,r#""([^"\\]*(\\.[^"\\]*)*)""#));
        map.push((TokenKind::LetToken,r"let"));
        map.push((TokenKind::ReturnToken,r"return"));
        map.push((TokenKind::BreakToken,r"break"));
        map.push((TokenKind::ContinueToken,r"continue"));


        map.push((TokenKind::IfToken,r"fun"));
        map.push((TokenKind::NumberToken,r"[0-9]+(\.[0-9]+)?"));

        map.push((TokenKind::EqualEqualToken,r"=="));
        map.push((TokenKind::EqualToken,r"="));
        map.push((TokenKind::NotEqualToken,r"!="));
        map.push((TokenKind::SmallerThanToken,r"<"));
        map.push((TokenKind::SmallerThanEqualToken,r"<="));
        map.push((TokenKind::GreaterThanToken,r">"));
        map.push((TokenKind::GreaterThanEqualToken,r">="));

        map.push((TokenKind::SemicolonToken,r";"));
        map.push((TokenKind::ColonToken,r":"));
        map.push((TokenKind::CommaToken,r","));
        map.push((TokenKind::DotToken,r"\."));

        map.push((TokenKind::PlusToken,r"\+"));
        map.push((TokenKind::MinusToken,r"\-"));
        map.push((TokenKind::StarToken,r"\*"));
        map.push((TokenKind::SlashToken,r"/"));

        map.push((TokenKind::OpenParenthesisToken,r"\("));
        map.push((TokenKind::CloseParenthesisToken,r"\)"));
        map.push((TokenKind::CurlyOpenBracketToken,r"\{"));
        map.push((TokenKind::CurlyCloseBracketToken,r"\}"));

        map.push((TokenKind::WhiteSpaceToken,r"\s+"));

        map.push((TokenKind::IdentifierToken,"[a-zA-Z_][a-zA-Z0-9_]*"));
        return map;
    }

    //get all token
    pub fn lex_all(&mut self)->Vec<SyntaxToken>
    {
        self.diagnostics.clear();
        let mut res=vec![];
        loop {
            let c=self.next_token();
            if c.kind==TokenKind::BadToken
            {
                self.diagnostics.push(format!("unexpected token '{}' at {}",c.text,c.position.get_point_str()));
                continue;
            }
            else if c.kind==TokenKind::WhiteSpaceToken
            {
                continue;
            }
            else if c.kind==TokenKind::EndOfFileToken
            {
                break;
            }
            res.push(c);
        }
        res
    }

    //return current character if it is not in index range then returns end of file character
    fn current_char(&self) -> char {
        if self.current < self.input_text.len() {
            self.input_text.chars().nth(self.current).unwrap()
        } else {
            '\0'
        }
    }
    //returns string sliced from current position returns EOF
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

    ///match the current string with the regex and return the token if it is valid otherwise return None
    fn do_match(regex_str:&str,token:TokenKind,current:&mut usize,current_str:&String,line_text:&LineText)->Option<SyntaxToken>
    {
        let re=regex::Regex::new(regex_str).unwrap(); //ugly workaround should probably cache the regex
        for cap in re.captures_iter(current_str.as_str())
        {
            //if our first regex match does not start at the beginning of the string then we have a no match
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
