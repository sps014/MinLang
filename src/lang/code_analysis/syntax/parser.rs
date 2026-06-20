use std::io::Error;
use bumpalo::Bump;
use crate::lang::code_analysis::syntax::nodes::{ExpressionNode, FunctionNode, Type, ParameterNode, ProgramNode, StatementNode, ImportNode};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::line_text::LineText;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::code_analysis::token::token_kind::TokenKind::{EndOfFileToken, IdentifierToken};
use crate::lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::diagnostics::DiagnosticBag;

/// The parser is responsible for converting a sequence of tokens into an Abstract Syntax Tree (AST).
/// It uses a recursive descent parsing strategy.
pub struct Parser<'a, 'b>
{
    lexer:Lexer,
    tokens:Vec<SyntaxToken>,
    current_token_index:usize,
    arena: &'a Bump,
    diagnostics: &'b mut DiagnosticBag,
}

impl<'a, 'b> Parser<'a, 'b>
{
    ///creates a new instance of the parser from a lexer instance
    pub fn new(lexer:Lexer, arena: &'a Bump, diagnostics: &'b mut DiagnosticBag) -> Self
    {
        Self
        {
            lexer,
            tokens:Vec::new(),
            current_token_index:0,
            arena,
            diagnostics,
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
    ///checks if the current token is of the given kind, returns that token, moves to next token else synthesizes one and reports error
    fn match_token(&mut self,kind:TokenKind) -> SyntaxToken
    {
        let token=self.current_token();
        if token.kind==kind
        {
            self.next_token()
        }
        else
        {
            self.diagnostics.report_error(
                format!("Expected token of kind {:?} but found {:?}", kind, token.kind),
                Some(token.position.clone())
            );
            SyntaxToken::new(kind, token.position.clone(), "".to_string())
        }
    }
    ///parse all tokens from lexer and returns a syntax tree or error
    pub fn parse(&mut self)->Result<SyntaxTree<'a>,Error>
    {
        self.tokens = self.lexer.lex_all(self.diagnostics);
        Ok(SyntaxTree::new(self.parse_program()?))
    }

    ///get all functions in the file
    fn parse_program(&mut self)->Result<ProgramNode<'a>,Error>
    {
        let mut imports=vec![];
        let mut functions=vec![];
        
        while self.current_token().kind == TokenKind::ImportToken {
            imports.push(self.parse_import()?);
        }
        
        while self.current_token().kind!=TokenKind::EndOfFileToken
        {
            if self.current_token().kind == TokenKind::FunToken || self.current_token().kind == TokenKind::ExportToken {
                let function=self.parse_function()?;
                functions.push(function);
            } else {
                let cur = self.current_token();
                self.diagnostics.report_error(
                    format!("Expected function declaration but found {:?}", cur.kind),
                    Some(cur.position.clone())
                );
                self.next_token();
            }
        }
        Ok(ProgramNode::new(imports, functions))
    }
    
    /// Parses an import statement
    fn parse_import(&mut self)->Result<ImportNode,Error>
    {
        self.match_token(TokenKind::ImportToken);
        let module_name = self.match_token(TokenKind::StringToken);
        Ok(ImportNode::new(module_name))
    }
    /// Parses a function declaration
    fn parse_function(&mut self)->Result<FunctionNode<'a>,Error>
    {
        let mut is_exported = false;
        if self.current_token().kind == TokenKind::ExportToken {
            self.match_token(TokenKind::ExportToken);
            is_exported = true;
        }
        
        //eat the fun keyword
        self.match_token(TokenKind::FunToken);
        let function_name=self.match_token(TokenKind::IdentifierToken);
        let params=self.parse_formal_parameters()?;
        let mut return_type:Option<Type>=None;
        if self.current_token().kind==TokenKind::ColonToken
        {
            //eat the colon
            self.match_token(TokenKind::ColonToken);
            let type_r=self.match_token(TokenKind::DataTypeToken);
            return_type=Some(Type::from_token(type_r)?);
        }
        let block=self.parse_block()?;
        Ok(FunctionNode::new(function_name,return_type,params,block,is_exported))
    }
    /// Parses formal parameters for a function declaration
    fn parse_formal_parameters(&mut self)->Result<Vec<ParameterNode>,Error>
    {
        let mut params=vec![];
        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken);

        while self.current_token().kind != TokenKind::CloseParenthesisToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
           //eat the identifier
           let param=self.match_token(TokenKind::IdentifierToken);
            //eat the colon
            self.match_token(TokenKind::ColonToken);
            //eat the type
            let param_type=self.match_token(TokenKind::DataTypeToken);
            params.push(ParameterNode::new(param,param_type));
            //if we have comma and it is not trailing comma
            if self.current_token().kind==TokenKind::CommaToken
            {
                //next token of comma is identifier eat comma then
                if self.peek_token(1).kind==TokenKind::IdentifierToken
                {
                    //eat the comma
                    self.match_token(TokenKind::CommaToken);
                }
            }
        }

        //eat the close parenthesis
        self.match_token(TokenKind::CloseParenthesisToken);
        Ok(params)
    }

    /// Parses a block of statements enclosed in curly braces
    fn parse_block(&mut self)->Result<&'a [StatementNode<'a>],Error>
    {
        //eat the open curly brace
        self.match_token(TokenKind::CurlyOpenBracketToken);
        let mut statements=vec![];
        while self.current_token().kind!=TokenKind::CurlyCloseBracketToken
            && self.current_token().kind!=TokenKind::EndOfFileToken
        {
            let statement=self.parse_statement()?;
            statements.push(statement);
        }
        //eat the close curly brace
        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(self.arena.alloc_slice_fill_iter(statements))
    }
    /// Parses a single statement based on the current token
    fn parse_statement(&mut self)->Result<StatementNode<'a>,Error>
    {
        let cur = self.current_token();
        match cur.kind {
            TokenKind::LetToken => Ok(self.parse_declaration()?),
            TokenKind::ReturnToken => Ok(self.parse_return()?),
            TokenKind::IfToken => Ok(self.parse_if_else()?),
            TokenKind::WhileToken => Ok(self.parse_while()?),
            TokenKind::ForToken => Ok(self.parse_for()?),
            TokenKind::BreakToken => Ok(self.parse_break()?),
            TokenKind::ContinueToken => Ok(self.parse_continue()?),
            TokenKind::IdentifierToken => {
                if self.peek_token(1).kind == TokenKind::EqualToken {
                    Ok(self.parse_assignment()?)
                } else if self.peek_token(1).kind == TokenKind::OpenParenthesisToken {
                    let r = self.parse_invocation_expression()?;
                    //eat the semicolon
                    self.match_token(TokenKind::SemicolonToken);
                    match r {
                        ExpressionNode::FunctionCall(name, params) => {
                            Ok(StatementNode::FunctionInvocation(name, params))
                        },
                        _ => {
                            self.diagnostics.report_error(
                                format!("Expected function call but found {:?}", r),
                                Some(cur.position.clone())
                            );
                            // Recover by returning a dummy statement
                            Ok(StatementNode::Break) 
                        }
                    }
                } else {
                    self.diagnostics.report_error(
                        format!("Unexpected identifier {:?} at {}", cur.text, cur.position.get_point_str()),
                        Some(cur.position.clone())
                    );
                    self.next_token(); // skip the token
                    Ok(StatementNode::Break) // dummy
                }
            },
            _ => {
                self.diagnostics.report_error(
                    format!("Expected statement but found {:?} at {}", cur.text, cur.position.get_point_str()),
                    Some(cur.position.clone())
                );
                self.next_token(); // skip the token
                Ok(StatementNode::Break) // dummy
            }
        }
    }

    /// Parses a variable declaration (e.g., `let x = 5;`)
    fn parse_declaration(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the keyword let
        self.match_token(TokenKind::LetToken);
        let identifier=self.match_token(TokenKind::IdentifierToken);
        //eat the equal sign
        self.match_token(TokenKind::EqualToken);
        let expression=self.parse_expression(0)?;
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Declaration(identifier,expression))
    }

    /// Parses a variable assignment (e.g., `x = 5;`)
    fn parse_assignment(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the identifier
        let identifier=self.match_token(TokenKind::IdentifierToken);
        //eat the equal sign
        self.match_token(TokenKind::EqualToken);
        let expression=self.parse_expression(0)?;
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Assignment(identifier,expression))
    }
    /// Parses an expression with operator precedence
    fn parse_expression(&mut self,parent_precedence:i32)->Result<ExpressionNode<'a>,Error>
    {
        let mut left;
        let unary_precedence = self.current_token().kind.get_unary_precedence();
        if unary_precedence != 0 && unary_precedence >= parent_precedence {
            let operator_token = self.next_token();
            let operand = self.parse_expression(unary_precedence)?;
            left = ExpressionNode::Unary(operator_token, self.arena.alloc(operand));
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
            left = ExpressionNode::Binary(self.arena.alloc(left),
                                          operator_token, self.arena.alloc(right));
        }
        Ok(left)
    }
    /// Parses a primary expression (literal, identifier, parenthesized expression, or function call)
    fn parse_primary_expression(&mut self)->Result<ExpressionNode<'a>,Error>
    {
        //parse parenthesized expressions
        if self.current_token().kind==TokenKind::OpenParenthesisToken
        {
            //eat the open parenthesis
            self.match_token(TokenKind::OpenParenthesisToken);
            let expression=self.parse_expression(0)?;
            //eat the close parenthesis
            self.match_token(TokenKind::CloseParenthesisToken);
            return Ok(ExpressionNode::Parenthesized(self.arena.alloc(expression)));
        }
        else if  self.current_token().kind==TokenKind::BooleanToken
        {
            return Ok(ExpressionNode::Literal(Type::Boolean(self.match_token(TokenKind::BooleanToken))));
        }
        else if self.current_token().kind==TokenKind::BooleanToken {
            return Ok(ExpressionNode::Literal(Type::Boolean(self.match_token(TokenKind::BooleanToken))));
        }
        //parse identifiers
        else if self.current_token().kind==IdentifierToken
        {
            if self.peek_token(1).kind==TokenKind::OpenParenthesisToken
            {
                return Ok(self.parse_invocation_expression()?);
            }
            else
            {
                return Ok(ExpressionNode::Identifier(self.next_token()));
            }
        }
        else if self.current_token().kind==TokenKind::NumberToken
        {
            if self.current_token().text.contains('.')
            {
                return Ok(ExpressionNode::Literal(Type::Float(self.next_token())));
            }
            else {
                return Ok(ExpressionNode::Literal(Type::Integer(self.next_token())));
            }
        }
        else if self.current_token().kind==TokenKind::StringToken
        {
            return Ok(ExpressionNode::Literal(Type::String(self.next_token())));
        }

        let identifier=self.match_token(TokenKind::IdentifierToken);
        Ok(ExpressionNode::Identifier(identifier))
    }
    /// Parses a function invocation expression
    fn parse_invocation_expression(&mut self)->Result<ExpressionNode<'a>,Error>
    {
        let function_name=self.match_token(TokenKind::IdentifierToken);
        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken);
        let mut arguments=Vec::new();
        while self.current_token().kind!=TokenKind::CloseParenthesisToken && self.current_token().kind!=EndOfFileToken
        {
            //parse the argument
            let argument=self.parse_expression(0)?;
            arguments.push(argument);
            if self.current_token().kind==TokenKind::CommaToken && self.peek_token(1).kind!=TokenKind::CloseParenthesisToken
            {
                //eat the comma
                self.match_token(TokenKind::CommaToken);
            }
        }
        //eat the close parenthesis
        self.match_token(TokenKind::CloseParenthesisToken);
        Ok(ExpressionNode::FunctionCall(function_name,arguments))
    }
    /// Parses a return statement
    fn parse_return(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the return keyword
        self.match_token(TokenKind::ReturnToken);
        let mut expression:Option<ExpressionNode>=None;
        if self.current_token().kind!=TokenKind::SemicolonToken
        {
            expression=Some(self.parse_expression(0)?);
        }

        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Return(expression))
    }
    /// Parses an if-else statement, including else-if chains
    fn parse_if_else(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the if keyword
        self.match_token(TokenKind::IfToken);
        let condition=self.parse_expression(0)?;
        let then_branch=self.parse_block()?;
        let mut else_ifs=vec![];
        while self.current_token().kind==TokenKind::ElseToken
        {
            //eat the else keyword
            self.match_token(TokenKind::ElseToken);
            if self.current_token().kind==TokenKind::IfToken
            {
                //eat the if keyword
                self.match_token(TokenKind::IfToken);
                let condition=self.parse_expression(0)?;
                let then_branch=self.parse_block()?;
                else_ifs.push((condition,then_branch));
            }
            else
            {
                let else_branch=self.parse_block()?;
                return Ok(StatementNode::IfElse(condition,then_branch,else_ifs,Some(else_branch)));
            }
        }

        Ok(StatementNode::IfElse(condition,then_branch,else_ifs,None))
    }

    /// Parses a for loop statement
    fn parse_for(&mut self)->Result<StatementNode<'a>,Error>
    {
        self.match_token(TokenKind::ForToken);
        let mut init: Option<&'a StatementNode<'a>> = None;
        if self.current_token().kind != TokenKind::SemicolonToken {
            if self.current_token().kind == TokenKind::LetToken {
                init = Some(self.arena.alloc(self.parse_declaration()?));
            } else {
                init = Some(self.arena.alloc(self.parse_assignment()?));
            }
        } else {
            self.match_token(TokenKind::SemicolonToken);
        }

        let mut condition = None;
        if self.current_token().kind != TokenKind::SemicolonToken {
            condition = Some(self.parse_expression(0)?);
        }
        self.match_token(TokenKind::SemicolonToken);

        let mut increment: Option<&'a StatementNode<'a>> = None;
        if self.current_token().kind != TokenKind::CurlyOpenBracketToken {
            let identifier=self.match_token(TokenKind::IdentifierToken);
            self.match_token(TokenKind::EqualToken);
            let expression=self.parse_expression(0)?;
            increment = Some(self.arena.alloc(StatementNode::Assignment(identifier,expression)));
        }

        let body=self.parse_block()?;
        Ok(StatementNode::For(init,condition,increment,body))
    }

    /// Parses a while loop statement
    fn parse_while(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the while keyword
        self.match_token(TokenKind::WhileToken);
        let condition=self.parse_expression(0)?;
        let body=self.parse_block()?;
        Ok(StatementNode::While(condition,body))
    }
    /// Parses a break statement
    fn parse_break(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the break keyword
        self.match_token(TokenKind::BreakToken);
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Break)
    }
    /// Parses a continue statement
    fn parse_continue(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the continue keyword
        self.match_token(TokenKind::ContinueToken);
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Continue)
    }
}
