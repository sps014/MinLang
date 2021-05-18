use crate::lang::code_analysis::lexer::Lexer;
use crate::lang::code_analysis::syntax_kind::SyntaxKind;
use crate::lang::code_analysis::syntax_token::SyntaxToken;
use crate::lang::code_analysis::syntax_tree::SyntaxTree;

use super::syntax_node::SyntaxNode;

pub struct Parser {
    diagnostics: Vec<String>,
    current: usize,
    tokens: Vec<SyntaxToken>,
}
impl Parser {
    pub fn new(text: &str) -> Parser {
        let mut lex = Lexer::new(text);
        let mut tokens = vec![];
        loop {
            let t = lex.next_token();
            if t.kind != SyntaxKind::BadToken
                && t.kind != SyntaxKind::WhiteSpaceToken
                && t.kind != SyntaxKind::NewLineToken
            {
                tokens.push(t.clone());
            }
            if t.kind != SyntaxKind::EndOfFileToken {
                continue;
            }
            break;
        }
        Parser {
            diagnostics: vec![],
            current: 0,
            tokens,
        }
    }
    pub fn peek(&self, offset: usize) -> SyntaxToken {
        let l = offset + self.current;
        if l >= self.tokens.len() {
            return self.tokens[self.tokens.len() - 1].clone();
        }
        self.tokens[l].clone()
    }
    pub fn get_current(&self) -> SyntaxToken {
        self.peek(0)
    }
    pub fn next_token(&mut self) -> SyntaxToken {
        let c = self.get_current();
        self.next();
        return c;
    }
    pub fn next(&mut self) {
        self.current += 1;
    }
    fn match_token(&mut self, kind: SyntaxKind) -> SyntaxToken {
        if self.get_current().kind == kind {
            return self.next_token().clone();
        }
        self.diagnostics.push(format!(
            "ERROR: unexpected token <{}>",
            self.get_current().text
        ));
        return SyntaxToken::new(kind, self.current, "");
    }
     pub fn parse(&mut self)->SyntaxTree
     {
         let expresion = self.parse_term();
         let eof_token = self.match_token(SyntaxKind::EndOfFileToken);
         SyntaxTree::new(self.diagnostics.clone(), expresion, eof_token)
     }
     fn parse_expression(&mut self)-> Box<SyntaxNode>
     {
         self.parse_term()
     }
     fn parse_term(&mut self)-> Box<SyntaxNode>
     {
         let mut left=self.parse_factor();
         while self.get_current().kind == SyntaxKind::PlusToken ||
             self.get_current().kind == SyntaxKind::MinusToken
         {
             let operator_token = self.next_token();
             let right = self.parse_factor();
             left = Box::new(SyntaxNode::BinaryExpressionSyntax(left, operator_token, right));
         }

         return left;
     }
    fn parse_factor(&mut self)->Box< SyntaxNode>
    {
        let mut left = self.parse_primary_expression();

        while self.get_current().kind == SyntaxKind::StarToken ||
            self.get_current().kind == SyntaxKind::SlashToken
        {
            let operator_token = self.next_token();
            let right = self.parse_primary_expression();
            left= Box::new(SyntaxNode::BinaryExpressionSyntax(left, operator_token, right));
        }

        return left;
    }
    fn parse_primary_expression(&mut self)->Box<SyntaxNode>
    {
        if self.get_current().kind==SyntaxKind::OpenParenthesisToken
        {
            let left = self.next_token();
            let expression = self.parse_expression();
            let right = self.match_token(SyntaxKind::CloseParenthesisToken);
            return  Box::new(SyntaxNode::ParenthesizedExpressionSyntax(left,expression,right));
        }
        let number_token = self.match_token(SyntaxKind::NumberToken);
        return Box::new(SyntaxNode::NumberExpressionSyntax(number_token));
    }
}
