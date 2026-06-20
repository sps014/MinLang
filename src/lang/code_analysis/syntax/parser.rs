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
        let mut structs=vec![];
        
        while self.current_token().kind == TokenKind::ImportToken {
            imports.push(self.parse_import()?);
        }
        
        while self.current_token().kind!=TokenKind::EndOfFileToken
        {
            if self.current_token().kind == TokenKind::StructToken || (self.current_token().kind == TokenKind::ExportToken && self.peek_token(1).kind == TokenKind::StructToken) {
                let struct_decl = self.parse_struct_declaration()?;
                structs.push(struct_decl);
            } else if self.current_token().kind == TokenKind::FunToken || (self.current_token().kind == TokenKind::ExportToken && self.peek_token(1).kind == TokenKind::FunToken) {
                let function=self.parse_function()?;
                functions.push(function);
            } else {
                let cur = self.current_token();
                self.diagnostics.report_error(
                    format!("Expected function or struct declaration but found {:?}", cur.kind),
                    Some(cur.position.clone())
                );
                self.next_token();
            }
        }
        Ok(ProgramNode::new(imports, structs, functions))
    }
    
    /// Parses a struct declaration
    fn parse_struct_declaration(&mut self) -> Result<crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>, Error> {
        let mut is_exported = false;
        if self.current_token().kind == TokenKind::ExportToken {
            self.match_token(TokenKind::ExportToken);
            is_exported = true;
        }
        
        self.match_token(TokenKind::StructToken);
        let struct_name = self.match_token(TokenKind::IdentifierToken);

        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                params.push(self.match_token(TokenKind::IdentifierToken));
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
            }
            self.match_token(TokenKind::GreaterThanToken);
            generic_parameters = Some(params);
        }

        self.match_token(TokenKind::CurlyOpenBracketToken);
        
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken && self.current_token().kind != TokenKind::EndOfFileToken {
            if self.current_token().kind == TokenKind::FunToken || self.current_token().kind == TokenKind::ExportToken {
                methods.push(self.parse_function()?);
            } else {
                let field_name = self.match_token(TokenKind::IdentifierToken);
                self.match_token(TokenKind::ColonToken);
                
                let mut field_type_token = if self.current_token().kind == TokenKind::DataTypeToken {
                    self.match_token(TokenKind::DataTypeToken)
                } else {
                    self.match_token(TokenKind::IdentifierToken)
                };
                
                while self.current_token().kind == TokenKind::OpenBracketToken {
                    self.match_token(TokenKind::OpenBracketToken);
                    self.match_token(TokenKind::CloseBracketToken);
                    field_type_token.text.push_str("[]");
                }
                
                if self.current_token().kind == TokenKind::QuestionMarkToken {
                    self.match_token(TokenKind::QuestionMarkToken);
                    field_type_token.text.push_str("?");
                }
                
                self.match_token(TokenKind::SemicolonToken);
                fields.push(crate::lang::code_analysis::syntax::nodes::struct_node::StructFieldNode {
                    name: field_name,
                    type_token: field_type_token,
                });
            }
        }
        
        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode::new(struct_name, generic_parameters, fields, methods, is_exported))
    }
    
    /// Parses an import statement
    fn parse_import(&mut self)->Result<ImportNode,Error>
    {
        self.match_token(TokenKind::ImportToken);
        let module_name = self.match_token(TokenKind::StringToken);
        Ok(ImportNode::new(module_name))
    }
    /// Parses a Type from the token stream, including array types
    fn parse_type(&mut self) -> Result<Type, Error> {
        let type_token = if self.current_token().kind == TokenKind::DataTypeToken {
            self.match_token(TokenKind::DataTypeToken)
        } else {
            self.match_token(TokenKind::IdentifierToken)
        };
        let mut parsed_type = Type::from_token(type_token)?;
        
        // Check for generic arguments
        if let Type::Struct(token, _) = &parsed_type {
            if self.current_token().kind == TokenKind::SmallerThanToken {
                self.match_token(TokenKind::SmallerThanToken);
                let mut args = Vec::new();
                while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                    args.push(self.parse_type()?);
                    if self.current_token().kind == TokenKind::CommaToken {
                        self.match_token(TokenKind::CommaToken);
                    }
                }
                self.match_token(TokenKind::GreaterThanToken);
                parsed_type = Type::Struct(token.clone(), Some(args));
            }
        }
        
        // Check for array suffix `[]`
        while self.current_token().kind == TokenKind::OpenBracketToken {
            self.match_token(TokenKind::OpenBracketToken);
            self.match_token(TokenKind::CloseBracketToken);
            parsed_type = Type::Array(Box::new(parsed_type));
        }
        
        // Check for nullable suffix `?`
        if self.current_token().kind == TokenKind::QuestionMarkToken {
            self.match_token(TokenKind::QuestionMarkToken);
            parsed_type = Type::Nullable(Box::new(parsed_type));
        }
        
        Ok(parsed_type)
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
        
        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                params.push(self.match_token(TokenKind::IdentifierToken));
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
            }
            self.match_token(TokenKind::GreaterThanToken);
            generic_parameters = Some(params);
        }

        let params=self.parse_formal_parameters()?;
        let mut return_type:Option<Type>=None;
        if self.current_token().kind==TokenKind::ColonToken
        {
            //eat the colon
            self.match_token(TokenKind::ColonToken);
            return_type=Some(self.parse_type()?);
        }
        let block=self.parse_block()?;
        Ok(FunctionNode::new(function_name,generic_parameters,return_type,params,block,is_exported))
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
            
            let param_type = self.parse_type()?;
            params.push(ParameterNode::new(param, param_type));
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
                // Parse an expression first
                let expr = self.parse_primary_expression()?;
                
                if self.current_token().kind == TokenKind::EqualToken {
                    self.match_token(TokenKind::EqualToken);
                    let value = self.parse_expression(0)?;
                    self.match_token(TokenKind::SemicolonToken);
                    
                    match expr {
                        ExpressionNode::Identifier(id) => Ok(StatementNode::Assignment(id, value)),
                        ExpressionNode::IndexAccess(arr, idx) => Ok(StatementNode::IndexAssignment(arr, idx, value)),
                        ExpressionNode::MemberAccess(obj, member) => Ok(StatementNode::MemberAssignment(obj, member, value)),
                        _ => {
                            self.diagnostics.report_error(
                                format!("Invalid assignment target"),
                                Some(cur.position.clone())
                            );
                            Ok(StatementNode::Break)
                        }
                    }
                } else if self.current_token().kind == TokenKind::SemicolonToken {
                    self.match_token(TokenKind::SemicolonToken);
                    match expr {
                        ExpressionNode::FunctionCall(name, generic_args, params) => {
                            Ok(StatementNode::FunctionInvocation(name, generic_args, params))
                        },
                        ExpressionNode::MethodCall(obj, member, generic_args, params) => {
                            Ok(StatementNode::MethodInvocation(obj, member, generic_args, params))
                        },
                        _ => {
                            self.diagnostics.report_error(
                                format!("Expected function call but found expression"),
                                Some(cur.position.clone())
                            );
                            Ok(StatementNode::Break) 
                        }
                    }
                } else {
                    self.diagnostics.report_error(
                        format!("Unexpected token {:?} after expression", self.current_token().kind),
                        Some(self.current_token().position.clone())
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

    /// Parses a variable declaration (e.g., `let x = 5;` or `let x: int[] = [1];`)
    fn parse_declaration(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the keyword let
        self.match_token(TokenKind::LetToken);
        let identifier=self.match_token(TokenKind::IdentifierToken);
        
        // Optional type annotation
        let mut type_annotation = None;
        if self.current_token().kind == TokenKind::ColonToken {
            self.match_token(TokenKind::ColonToken);
            type_annotation = Some(self.parse_type()?);
        }
        
        //eat the equal sign
        self.match_token(TokenKind::EqualToken);
        let expression=self.parse_expression(0)?;
        //eat the semicolon
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Declaration(identifier, type_annotation, expression))
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
            if operator_token.kind == TokenKind::IsToken {
                let right_type = self.parse_type()?;
                left = ExpressionNode::IsExpression(self.arena.alloc(left), right_type);
            } else {
                let right = self.parse_expression(precedence)?;
                left = ExpressionNode::Binary(self.arena.alloc(left),
                                              operator_token, self.arena.alloc(right));
            }
        }
        Ok(left)
    }
    /// Parses a primary expression (literal, identifier, parenthesized expression, or function call)
    fn parse_primary_expression(&mut self)->Result<ExpressionNode<'a>,Error>
    {
        //parse parenthesized expressions or cast
        if self.current_token().kind==TokenKind::OpenParenthesisToken
        {
            let is_cast = if self.peek_token(1).kind == TokenKind::DataTypeToken {
                true
            } else if self.peek_token(1).kind == TokenKind::IdentifierToken {
                // Could be `(Node)0` or `(x) + 1`
                // Let's check token after `)`
                let mut i = 2;
                while self.peek_token(i).kind == TokenKind::OpenBracketToken {
                    i += 2; // skip `[` and `]`
                }
                if self.peek_token(i).kind == TokenKind::CloseParenthesisToken {
                    let next_kind = self.peek_token(i + 1).kind;
                    // If the token after `)` is an expression starter, it's a cast
                    match next_kind {
                        TokenKind::NumberToken | TokenKind::StringToken | TokenKind::BooleanToken |
                        TokenKind::IdentifierToken | TokenKind::OpenParenthesisToken | TokenKind::OpenBracketToken |
                        TokenKind::MinusToken | TokenKind::BangToken => true,
                        _ => false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if is_cast {
                self.match_token(TokenKind::OpenParenthesisToken);
                let cast_type = self.parse_type()?;
                self.match_token(TokenKind::CloseParenthesisToken);
                let expression = self.parse_primary_expression()?;
                return Ok(ExpressionNode::Cast(cast_type, self.arena.alloc(expression)));
            }
            
            //eat the open parenthesis
            self.match_token(TokenKind::OpenParenthesisToken);
            let expression=self.parse_expression(0)?;
            //eat the close parenthesis
            self.match_token(TokenKind::CloseParenthesisToken);
            return Ok(ExpressionNode::Parenthesized(self.arena.alloc(expression)));
        }
        else if self.current_token().kind==TokenKind::OpenBracketToken {
            // Array literal
            self.match_token(TokenKind::OpenBracketToken);
            let mut elements = Vec::new();
            while self.current_token().kind != TokenKind::CloseBracketToken && self.current_token().kind != TokenKind::EndOfFileToken {
                elements.push(self.parse_expression(0)?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
            }
            self.match_token(TokenKind::CloseBracketToken);
            return Ok(ExpressionNode::ArrayLiteral(elements));
        }
        else if  self.current_token().kind==TokenKind::BooleanToken
        {
            return Ok(ExpressionNode::Literal(Type::Boolean(self.match_token(TokenKind::BooleanToken))));
        }
        else if self.current_token().kind==TokenKind::NullToken {
            self.match_token(TokenKind::NullToken);
            // `Nullable(Void)` represents the `null` literal until its concrete type is known.
            return Ok(ExpressionNode::Literal(Type::Nullable(Box::new(Type::Void))));
        }
        //parse identifiers
        else if self.current_token().kind==IdentifierToken
        {
            let mut is_invocation = false;
            let mut is_struct_instantiation = false;
            
            if self.peek_token(1).kind==TokenKind::OpenParenthesisToken {
                is_invocation = true;
            } else if self.peek_token(1).kind==TokenKind::CurlyOpenBracketToken {
                is_struct_instantiation = true;
            } else if self.peek_token(1).kind == TokenKind::SmallerThanToken {
                // Check if it's a generic invocation like `Test<int>(...)` or `Box<int> { ... }`
                let mut i = 2;
                while self.peek_token(i).kind != TokenKind::EndOfFileToken {
                    if self.peek_token(i).kind == TokenKind::GreaterThanToken {
                        if self.peek_token(i + 1).kind == TokenKind::OpenParenthesisToken {
                            is_invocation = true;
                        } else if self.peek_token(i + 1).kind == TokenKind::CurlyOpenBracketToken {
                            is_struct_instantiation = true;
                        }
                        break;
                    }
                    if self.peek_token(i).kind == TokenKind::SemicolonToken || self.peek_token(i).kind == TokenKind::CurlyOpenBracketToken {
                        break;
                    }
                    i += 1;
                }
            }

            if is_invocation
            {
                return Ok(self.parse_invocation_expression()?);
            }
            else if is_struct_instantiation
            {
                // Struct instantiation: Point { x: 10, y: 20 } or Box<int> { val: 42 }
                let struct_name = self.match_token(TokenKind::IdentifierToken);
                
                let mut generic_arguments = None;
                if self.current_token().kind == TokenKind::SmallerThanToken {
                    self.match_token(TokenKind::SmallerThanToken);
                    let mut args = Vec::new();
                    while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                        args.push(self.parse_type()?);
                        if self.current_token().kind == TokenKind::CommaToken {
                            self.match_token(TokenKind::CommaToken);
                        }
                    }
                    self.match_token(TokenKind::GreaterThanToken);
                    generic_arguments = Some(args);
                }
                
                self.match_token(TokenKind::CurlyOpenBracketToken);
                let mut fields = Vec::new();
                while self.current_token().kind != TokenKind::CurlyCloseBracketToken && self.current_token().kind != TokenKind::EndOfFileToken {
                    let field_name = self.match_token(TokenKind::IdentifierToken);
                    self.match_token(TokenKind::ColonToken);
                    let field_value = self.parse_expression(0)?;
                    fields.push((field_name, field_value));
                    if self.current_token().kind == TokenKind::CommaToken {
                        self.match_token(TokenKind::CommaToken);
                    }
                }
                self.match_token(TokenKind::CurlyCloseBracketToken);
                return Ok(ExpressionNode::StructInstantiation(struct_name, generic_arguments, fields));
            }
            else
            {
                let mut expr = ExpressionNode::Identifier(self.next_token());
                
                // Check for index access or member access
                loop {
                    if self.current_token().kind == TokenKind::OpenBracketToken {
                        self.match_token(TokenKind::OpenBracketToken);
                        let index = self.parse_expression(0)?;
                        self.match_token(TokenKind::CloseBracketToken);
                        expr = ExpressionNode::IndexAccess(
                            self.arena.alloc(expr),
                            self.arena.alloc(index)
                        );
                    } else if self.current_token().kind == TokenKind::DotToken {
                        self.match_token(TokenKind::DotToken);
                        let member = self.match_token(TokenKind::IdentifierToken);
                        
                        let mut generic_args = None;
                        if self.current_token().kind == TokenKind::SmallerThanToken {
                            // Method generic args
                            let mut i = 1;
                            let mut is_generic = false;
                            while self.peek_token(i).kind != TokenKind::EndOfFileToken {
                                if self.peek_token(i).kind == TokenKind::GreaterThanToken {
                                    if self.peek_token(i + 1).kind == TokenKind::OpenParenthesisToken {
                                        is_generic = true;
                                    }
                                    break;
                                }
                                if self.peek_token(i).kind == TokenKind::SemicolonToken || self.peek_token(i).kind == TokenKind::CurlyOpenBracketToken {
                                    break;
                                }
                                i += 1;
                            }
                            if is_generic {
                                self.match_token(TokenKind::SmallerThanToken);
                                let mut args = Vec::new();
                                while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                                    args.push(self.parse_type()?);
                                    if self.current_token().kind == TokenKind::CommaToken {
                                        self.match_token(TokenKind::CommaToken);
                                    }
                                }
                                self.match_token(TokenKind::GreaterThanToken);
                                generic_args = Some(args);
                            }
                        }
                        
                        if self.current_token().kind == TokenKind::OpenParenthesisToken {
                            self.match_token(TokenKind::OpenParenthesisToken);
                            let mut params = Vec::new();
                            while self.current_token().kind != TokenKind::CloseParenthesisToken && self.current_token().kind != TokenKind::EndOfFileToken {
                                params.push(self.parse_expression(0)?);
                                if self.current_token().kind == TokenKind::CommaToken {
                                    self.match_token(TokenKind::CommaToken);
                                }
                            }
                            self.match_token(TokenKind::CloseParenthesisToken);
                            
                            expr = ExpressionNode::MethodCall(
                                self.arena.alloc(expr),
                                member,
                                generic_args,
                                params
                            );
                        } else {
                            expr = ExpressionNode::MemberAccess(
                                self.arena.alloc(expr),
                                member
                            );
                        }
                    } else {
                        break;
                    }
                }
                
                return Ok(expr);
            }
        }
        else if self.current_token().kind==TokenKind::NumberToken
        {
            let text = self.current_token().text.clone();
            if text.ends_with('d') || text.ends_with('D') {
                let mut token = self.next_token();
                token.text = token.text[..token.text.len() - 1].to_string();
                return Ok(ExpressionNode::Literal(Type::Double(token)));
            } else if text.ends_with('f') || text.ends_with('F') {
                let mut token = self.next_token();
                token.text = token.text[..token.text.len() - 1].to_string();
                return Ok(ExpressionNode::Literal(Type::Float(token)));
            } else if text.contains('.') {
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

        let cur = self.current_token();
        if cur.kind != TokenKind::IdentifierToken {
            self.diagnostics.report_error(
                format!("Expected expression but found {:?}", cur.kind),
                Some(cur.position.clone())
            );
            self.next_token(); // skip the unexpected token to avoid infinite loop
            return Ok(ExpressionNode::Identifier(SyntaxToken::new(TokenKind::IdentifierToken, cur.position.clone(), "".to_string())));
        }

        let identifier=self.match_token(TokenKind::IdentifierToken);
        Ok(ExpressionNode::Identifier(identifier))
    }
    /// Parses a function invocation expression
    fn parse_invocation_expression(&mut self)->Result<ExpressionNode<'a>,Error>
    {
        let function_name=self.match_token(TokenKind::IdentifierToken);
        
        let mut generic_arguments = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut args = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                args.push(self.parse_type()?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
            }
            self.match_token(TokenKind::GreaterThanToken);
            generic_arguments = Some(args);
        }

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
        Ok(ExpressionNode::FunctionCall(function_name, generic_arguments, arguments))
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
        self.match_token(TokenKind::OpenParenthesisToken);
        let condition=self.parse_expression(0)?;
        self.match_token(TokenKind::CloseParenthesisToken);
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
                self.match_token(TokenKind::OpenParenthesisToken);
                let condition=self.parse_expression(0)?;
                self.match_token(TokenKind::CloseParenthesisToken);
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
        self.match_token(TokenKind::OpenParenthesisToken);
        let mut init: Option<&'a StatementNode<'a>> = None;
        if self.current_token().kind != TokenKind::SemicolonToken {
            if self.current_token().kind == TokenKind::LetToken {
                init = Some(self.arena.alloc(self.parse_declaration()?));
            } else {
                init = Some(self.arena.alloc(self.parse_statement()?));
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
        if self.current_token().kind != TokenKind::CloseParenthesisToken {
            // Parse the increment assignment expression (without semicolon)
            let expr = self.parse_primary_expression()?;
            self.match_token(TokenKind::EqualToken);
            let value = self.parse_expression(0)?;
            
            let stmt = match expr {
                ExpressionNode::Identifier(id) => StatementNode::Assignment(id, value),
                ExpressionNode::IndexAccess(arr, idx) => StatementNode::IndexAssignment(arr, idx, value),
                ExpressionNode::MemberAccess(obj, member) => StatementNode::MemberAssignment(obj, member, value),
                _ => {
                    self.diagnostics.report_error(
                        format!("Invalid assignment target in for loop increment"),
                        Some(self.current_token().position.clone())
                    );
                    StatementNode::Break
                }
            };
            increment = Some(self.arena.alloc(stmt));
        }
        self.match_token(TokenKind::CloseParenthesisToken);

        let body=self.parse_block()?;
        Ok(StatementNode::For(init,condition,increment,body))
    }

    /// Parses a while loop statement
    fn parse_while(&mut self)->Result<StatementNode<'a>,Error>
    {
        //eat the while keyword
        self.match_token(TokenKind::WhileToken);
        self.match_token(TokenKind::OpenParenthesisToken);
        let condition=self.parse_expression(0)?;
        self.match_token(TokenKind::CloseParenthesisToken);
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

#[cfg(test)]
#[path = "tests/parser_tests.rs"]
mod tests;
