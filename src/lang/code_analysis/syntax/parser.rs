use std::io::{Error, ErrorKind};
use std::thread::park;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, NumberLiteral, ParameterNode, ProgramNode, StatementNode};
use crate::lang::code_analysis::text::line_text::LineText;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::code_analysis::token::token_kind::TokenKind::{EndOfFileToken, IdentifierToken};
use crate::Lexer;

pub struct Parser<'a>
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
    //returns the new eof token
    fn new_eof_token() -> SyntaxToken
    {
        SyntaxToken::new(TokenKind::EndOfFileToken,
                         TextSpan::new((0,0),
                                       &LineText::new("".to_string())),
                         "\0".to_string())
    }
    ///returns current token if exists or None
    fn current_token(&self) -> SyntaxToken
    {
        if self.current_token_index >= self.tokens.len()
        {
            Parser::new_eof_token()
        }
        else { self.tokens[self.current_token_index].clone() }
    }
    ///returns current token and moves to next token
    fn next_token(&mut self) -> SyntaxToken
    {
        let r=self.current_token();
        self.current_token_index += 1;
        r
    }
    ///return the token at the given index with some offset
    fn peek_token(&self,offset:usize) -> SyntaxToken
    {
        if self.current_token_index + offset >= self.tokens.len(){Parser::new_eof_token()}
        else { self.tokens[self.current_token_index + offset].clone() }
    }

    fn match_token(&mut self,kind:TokenKind) -> Result<SyntaxToken,Error>
    {
        let token=self.next_token();
        if token.kind==kind
        {
            Ok(token)
        }
        else
        {
            Err(Error::new(ErrorKind::Other,
                           format!("Expected token of kind {:?} but found {:?} at {}",kind,token.text,
                                   token.position.get_point_str())))
        }
    }
    fn match_token_str(&mut self,kind:TokenKind,val:&str) -> Result<SyntaxToken,Error>
    {
        let token=self.next_token();
        if token.kind==kind && token.text==val
        {
            Ok(token)
        }
        else
        {
            Err(Error::new(ErrorKind::Other,
                           format!("Expected token of kind {:?} but found {:?} at {}",kind,token.text,
                                   token.position.get_point_str())))
        }
    }
    pub fn match_data_type(token:&SyntaxToken)->Result<(String),Error>
    {
        if token.kind==TokenKind::KeywordToken
            && (token.text=="int" || token.text=="float")
        {
            Ok(token.text.to_string())
        }
        else {
            Err(Error::new(ErrorKind::Other,
                           format!("Expected token of kind {:?} but found {:?} at {}",token.kind,token.text,
                                   token.position.get_point_str())))
        }
    }

    pub fn parse(&mut self)->Result<ProgramNode,Error>
    {
        self.tokens = self.lexer.lex_all();
        dbg!(&self.tokens);
        self.parse_program()
    }

    //get all functions in the file
    fn parse_program(&mut self)->Result<ProgramNode,Error>
    {
        let mut functions=vec![];
        while self.current_token().kind!=TokenKind::EndOfFileToken
        {
            let mut function=self.parse_function()?;
            functions.push(function);
        }
        Ok(ProgramNode::new(functions))
    }
    fn parse_function(&mut self)->Result<FunctionNode,Error>
    {
        //eat the fun keyword
        self.match_token_str(TokenKind::KeywordToken,"fun")?;
        let function_name=self.match_token(TokenKind::IdentifierToken)?;
        let params=self.parse_formal_parameters()?;
        if self.current_token().kind==TokenKind::ColonToken
        {
            //eat the colon
            self.match_token(TokenKind::ColonToken)?;
            let return_type=self.match_token(TokenKind::KeywordToken)?;
            //check is return type is valid
            Parser::match_data_type(&return_type)?;
        }
        let block=self.parse_block()?;
        Ok(FunctionNode::new(function_name.text,String::new(),params,block))
    }
    fn parse_formal_parameters(&mut self)->Result<Vec<ParameterNode>,Error>
    {
        let mut params=vec![];
        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken)?;

        while self.current_token().kind != TokenKind::CloseParenthesisToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
           //eat the identifier
           let param=self.match_token(TokenKind::IdentifierToken)?;
            //eat the colon
            self.match_token(TokenKind::ColonToken)?;
            //eat the type
            let param_type=self.match_token(TokenKind::KeywordToken)?;
            //if param_type is valid data type
            Self::match_data_type(&param_type)?;
            params.push(ParameterNode::new(param.text,param_type.text));
            //if we have comma and it is not trailing comma
            if self.current_token().kind==TokenKind::CommaToken
            {
                //next token of comma is identifier eat comma then
                if self.peek_token(1).kind==TokenKind::IdentifierToken
                {
                    //eat the comma
                    self.match_token(TokenKind::CommaToken)?;
                }
            }
        }

        //eat the close parenthesis
        self.match_token(TokenKind::CloseParenthesisToken)?;
        Ok(params)
    }

    fn parse_block(&mut self)->Result<Vec<StatementNode>,Error>
    {
        //eat the open curly brace
        self.match_token(TokenKind::CurlyOpenBracketToken)?;
        let mut statements=vec![];
        while self.current_token().kind!=TokenKind::CurlyCloseBracketToken
            && self.current_token().kind!=TokenKind::EndOfFileToken
        {
            let statement=self.parse_statement()?;
            statements.push(statement);
        }
        //eat the close curly brace
        self.match_token(TokenKind::CurlyCloseBracketToken)?;
        Ok(statements)
    }
    fn parse_statement(&mut self)->Result<StatementNode,Error>
    {
        let cur=self.current_token();
        if cur.kind==TokenKind::KeywordToken
        {
            if cur.text=="let"
            {
                return Ok(self.parse_declaration()?);
            }
            else if cur.text=="return"
            {
                return Ok(self.parse_return()?);
            }
        }
        else if cur.kind==TokenKind::IdentifierToken
        {
            if self.peek_token(1).kind==TokenKind::EqualToken
            {
                return Ok(self.parse_assignment()?);
            }
            else if self.peek_token(1).kind==TokenKind::OpenParenthesisToken
            {
                let r=self.parse_invocation_expression()?;
                //eat the semicolon
                self.match_token(TokenKind::SemicolonToken)?;
                match r
                {
                    ExpressionNode::FunctionCall(name,params)=>
                    {
                        return Ok(StatementNode::FunctionInvocation(name,params));
                    },
                    _=>{}
                }
            }
        }

        Err(Error::new(ErrorKind::Other,
                       format!("Expected statement but found {:?} at {}",self.current_token().text,
                               self.current_token().position.get_point_str())))
    }
    fn parse_declaration(&mut self)->Result<StatementNode,Error>
    {
        //eat the keyword let
        self.match_token_str(TokenKind::KeywordToken,"let")?;
        let identifier=self.match_token(TokenKind::IdentifierToken)?;
        //eat the equal sign
        self.match_token(TokenKind::EqualToken)?;
        let expression=self.parse_expression(0)?;
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken)?;
        Ok(StatementNode::Declaration(identifier.text,expression))
    }
    fn parse_assignment(&mut self)->Result<StatementNode,Error>
    {
        //eat the identifier
        let identifier=self.match_token(TokenKind::IdentifierToken)?;
        //eat the equal sign
        self.match_token(TokenKind::EqualToken)?;
        let expression=self.parse_expression(0)?;
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken)?;
        Ok(StatementNode::Assignment(identifier.text,expression))
    }
    fn parse_expression(&mut self,parent_precedence:i32)->Result<ExpressionNode,Error>
    {
        let mut left;
        let unary_precedence = self.current_token().kind.get_unary_precedence();
        if unary_precedence != 0 && unary_precedence >= parent_precedence {
            let operator_token = self.next_token();
            let operand = self.parse_expression(unary_precedence)?;
            left = ExpressionNode::Unary(operator_token.text, Box::new(operand));
        } else {
            left = self.parse_primary_expression()?;
        }
        loop
        {
            let precedence = self.current_token().kind.get_binary_precedence();
            if precedence == 0 || precedence <= parent_precedence {
                break;
            }

            let operator_token = self.next_token();
            let right = self.parse_expression(precedence)?;
            left = ExpressionNode::Binary(Box::new(left),
                                          operator_token.text, Box::new(right));
        }
        Ok(left)
    }
    fn parse_primary_expression(&mut self)->Result<ExpressionNode,Error>
    {
        //parse parenthesized expressions
        if self.current_token().kind==TokenKind::OpenParenthesisToken
        {
            //eat the open parenthesis
            self.match_token(TokenKind::OpenParenthesisToken)?;
            let expression=self.parse_expression(0)?;
            //eat the close parenthesis
            self.match_token(TokenKind::CloseParenthesisToken)?;
            return Ok(ExpressionNode::Parathized(Box::new(expression)));
        }
        //parse identifiers
        if self.current_token().kind==IdentifierToken
        {
            if self.peek_token(1).kind==TokenKind::OpenParenthesisToken
            {
                return Ok(self.parse_invocation_expression()?);
            }
            else
            {
                return Ok(ExpressionNode::Identifier(self.next_token().text.clone()));
            }
        }
        else if self.current_token().kind==TokenKind::NumberToken
        {
            if self.current_token().text.contains('.')
            {
                return Ok(ExpressionNode::Number(NumberLiteral::Float(self.next_token().text.parse::<f64>().unwrap() as f32)));
            }
            else {
                return Ok(ExpressionNode::Number(NumberLiteral::Integer(self.next_token().text.parse::<i32>().unwrap())));
            }
        }

        let identifier=self.match_token(TokenKind::IdentifierToken)?;
        Ok(ExpressionNode::Identifier(identifier.text))
    }
    fn parse_invocation_expression(&mut self)->Result<ExpressionNode,Error>
    {
        let function_name=self.match_token(TokenKind::IdentifierToken)?;
        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken)?;
        let mut arguments=Vec::new();
        while self.current_token().kind!=TokenKind::CloseParenthesisToken && self.current_token().kind!=EndOfFileToken
        {
            //parse the argument
            let argument=self.parse_expression(0)?;
            arguments.push(argument);
            if self.current_token().kind==TokenKind::CommaToken && self.peek_token(1).kind!=TokenKind::CloseParenthesisToken
            {
                //eat the comma
                self.match_token(TokenKind::CommaToken)?;
            }
        }
        //eat the close parenthesis
        self.match_token(TokenKind::CloseParenthesisToken)?;
        Ok(ExpressionNode::FunctionCall(function_name.text,arguments))
    }
    fn parse_return(&mut self)->Result<StatementNode,Error>
    {
        //eat the return keyword
        self.match_token_str(TokenKind::KeywordToken,"return")?;
        if(self.current_token().kind==TokenKind::SemicolonToken)
        {
            //eat the semicolon
            self.match_token(TokenKind::SemicolonToken)?;
            return Ok(StatementNode::Return(None));
        }

        let expression=self.parse_expression(0)?;
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken)?;
        Ok(StatementNode::Return(Some(expression)))
    }
}
