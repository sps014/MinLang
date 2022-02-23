use std::thread::park;
use crate::lang::code_analysis::syntax::syntax_node::ProgramNode;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::Lexer;

struct Parser<'a>
{
    lexer:Lexer<'a>,
    tokens:Vec<SyntaxToken>,
    current_token_index:usize,
}

impl<'a> Parser<'a>
{
    ///creates a new instance of the parser from a lexer instance
    pub fn new(lexer:Lexer<'a>) -> Self
    {
        Self
        {
            lexer,
            tokens:Vec::new(),
            current_token_index:0,
        }
    }
    ///returns current token if exists or None
    fn current_token(&self) -> Option<SyntaxToken>
    {
        if self.current_token_index >= self.tokens.len(){None }
        else { Some(self.tokens[self.current_token_index].clone()) }
    }
    ///returns current token and moves to next token
    fn next_token(&mut self) -> Option<SyntaxToken>
    {
        let r=self.current_token();
        self.current_token_index += 1;
        r
    }
    ///return the token at the given index with some offset
    fn peek_token(&self,offset:usize) -> Option<SyntaxToken>
    {
        if self.current_token_index + offset >= self.tokens.len(){None }
        else { Some(self.tokens[self.current_token_index + offset].clone()) }
    }

    pub fn parse(&mut self)->ProgramNode
    {
        self.tokens = self.lexer.lex_all();
        self.parse_program()
    }
    
    //get all functions in the file
    fn parse_program(&mut self)->ProgramNode
    {
        let mut functions=vec![];
        
        return ProgramNode::new(functions);
    }

}
