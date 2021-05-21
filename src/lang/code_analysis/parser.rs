use std::string;

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
        return SyntaxToken::new(kind, self.current, "\0");
    }
     pub fn parse(&mut self)->SyntaxTree
     {
         let expresion = self.parse_assignment();
         let eof_token = self.match_token(SyntaxKind::EndOfFileToken);
         SyntaxTree::new(self.diagnostics.clone(), expresion, eof_token)
     }
     fn parse_assignment(&mut self)->Box<SyntaxNode>
     {
         if self.peek(0).kind==SyntaxKind::IdentifierToken && self.peek(1).kind==SyntaxKind::EqualToken
         {
                let  identifier_token = self.next_token();
                let  operator_token = self.next_token();
                let  right = self.parse_assignment();
                let n= SyntaxNode::AssignmentExpressionSyntax(identifier_token, operator_token,right);
                return  Box::new(n);
         }
         return self.parse_expression(0);
     }
     fn parse_expression(&mut self,parent_precedence:i32)-> Box<SyntaxNode>
     {
         let mut left;
         let unary_precedence=self.get_current().kind.get_unary_precedence();
         if unary_precedence != 0 && unary_precedence >= parent_precedence
         {
             let operator_token = self.next_token();
             let operand = self.parse_expression(unary_precedence);
             left = SyntaxNode::UnaryExpressionSyntax(operator_token, operand);
         }
         else
         {
             left = self.parse_primary_expression().as_ref().clone();
         }

         loop
         {
             let precedence = self.get_current().kind.get_binary_precedence();
             if precedence == 0 || precedence <= parent_precedence
             {
                break;
             }

             let operator_token = self.next_token();
             let right = self.parse_expression(precedence);
             left = SyntaxNode::BinaryExpressionSyntax(Box::new(left), operator_token, right);
         }

         return Box::new(left);

     }


    fn parse_primary_expression(&mut self)->Box<SyntaxNode>
    {
        if self.get_current().kind==SyntaxKind::OpenParenthesisToken
        {
            let left = self.next_token();
            let expression = self.parse_expression(0);
            let right = self.match_token(SyntaxKind::CloseParenthesisToken);
            return  Box::new(SyntaxNode::ParenthesizedExpressionSyntax(left,expression,right));
        }
        let mut number_token = self.match_token(SyntaxKind::NumberToken);
        if number_token.text =="\0" &&  self.get_current().kind==SyntaxKind::IdentifierToken
        {
            if  self.peek(1).kind==SyntaxKind::OpenParenthesisToken
            {
                let id=self.next_token();
                let open=self.next_token();
                let mut callers=vec![];
                while self.get_current().kind!=SyntaxKind::CloseParenthesisToken {
                    let cur=self.get_current();
                    if cur.kind==SyntaxKind::EndOfFileToken
                    {
                        break;
                    }
                    callers.push(self.parse_primary_expression());
                    
                }
                let close=self.next_token();
                return Box::new(SyntaxNode::FunctionCallExpression(id,open,callers,close));
            }
            else
             {
                number_token=self.match_token(SyntaxKind::IdentifierToken);
            }
        }
      
        return Box::new(SyntaxNode::NumberExpressionSyntax(number_token));
    }
}
