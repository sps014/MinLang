use crate::lang::code_analysis::syntax_token::SyntaxToken;
use crate::lang::code_analysis::lexer::Lexer;
use crate::lang::code_analysis::syntax_kind::SyntaxKind;
use std::borrow::Borrow;

pub struct Parser
{
    diagnostics:Vec<String>,
    current:usize,
    tokens:Vec<SyntaxToken>
}
impl  Parser
{
    pub fn new(text:&str)->Parser
    {
        let mut lex=Lexer::new(text);
        let mut tokens=vec![];
        loop {
            let t=lex.next_token();
            if t.kind!=SyntaxKind::BadToken && t.kind!=SyntaxKind::WhiteSpaceToken && t.kind!=SyntaxKind::NewLineToken {
                tokens.push(t.clone());
            }
            if t.kind!=SyntaxKind::EndOfFileToken{
                continue;
            }
            break;
        }
        Parser{diagnostics:vec![],current:0,tokens}
    }
}