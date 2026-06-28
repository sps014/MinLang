use std::io::Error;
use crate::syntax::nodes::{ExpressionNode, Type};
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::syntax::token::token_kind::TokenKind::{EndOfFileToken, IdentifierToken};
use super::Parser;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses an expression with operator precedence
    pub(super) fn parse_expression(&mut self,parent_precedence:i32)->Result<ExpressionNode<'a>,Error>
    {
        let mut left;
        let unary_precedence = self.current_token().kind.get_unary_precedence();
        if self.current_token().kind == TokenKind::AwaitToken {
            // `await <primary>` binds tightly to its operand so `await f() + 1` is `(await f()) + 1`.
            self.match_token(TokenKind::AwaitToken);
            let operand = self.parse_primary_expression()?;
            left = ExpressionNode::Await(self.arena.alloc(operand));
        } else if unary_precedence != 0 && unary_precedence >= parent_precedence {
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

        // Ternary `cond ? a : b` binds looser than any binary operator and is right-associative.
        // It is only recognized at the top of an expression (parent_precedence == 0) so operands
        // of binary operators do not greedily consume a trailing `?`.
        if parent_precedence == 0 && self.current_token().kind == TokenKind::QuestionMarkToken {
            self.match_token(TokenKind::QuestionMarkToken);
            let then_expr = self.parse_expression(0)?;
            self.match_token(TokenKind::ColonToken);
            let else_expr = self.parse_expression(0)?;
            left = ExpressionNode::Ternary(
                self.arena.alloc(left),
                self.arena.alloc(then_expr),
                self.arena.alloc(else_expr),
            );
        }

        Ok(left)
    }
    /// Parses a primary expression (literal, identifier, parenthesized expression, or function call)
    pub(super) fn parse_primary_expression(&mut self)->Result<ExpressionNode<'a>,Error>
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
                let iter = self.current_token_index;
                elements.push(self.parse_expression(0)?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
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
        // A primitive type name used as a static-call receiver, e.g. `int.parse("5")`. The
        // keyword is treated as an identifier so the member/method-access loop below applies;
        // static dispatch is resolved later by the analyzer/codegen.
        else if self.current_token().kind==TokenKind::DataTypeToken
            && self.peek_token(1).kind==TokenKind::DotToken
        {
            let mut expr = ExpressionNode::Identifier(self.next_token());
            while self.current_token().kind == TokenKind::DotToken {
                self.match_token(TokenKind::DotToken);
                let member = self.match_token(TokenKind::IdentifierToken);
                let mut generic_args = None;
                if self.current_token().kind == TokenKind::SmallerThanToken {
                    let is_generic = self.scan_generic_args(1)
                        .map(|after| self.peek_token(after).kind == TokenKind::OpenParenthesisToken)
                        .unwrap_or(false);
                    if is_generic {
                        self.match_token(TokenKind::SmallerThanToken);
                        generic_args = Some(self.parse_generic_args()?);
                    }
                }
                if self.current_token().kind == TokenKind::OpenParenthesisToken {
                    self.match_token(TokenKind::OpenParenthesisToken);
                    let mut params = Vec::new();
                    while self.current_token().kind != TokenKind::CloseParenthesisToken && self.current_token().kind != TokenKind::EndOfFileToken {
                        let iter = self.current_token_index;
                        params.push(self.parse_expression(0)?);
                        if self.current_token().kind == TokenKind::CommaToken {
                            self.match_token(TokenKind::CommaToken);
                        }
                        self.ensure_progress(iter);
                    }
                    self.match_token(TokenKind::CloseParenthesisToken);
                    expr = ExpressionNode::MethodCall(self.arena.alloc(expr), member, generic_args, params);
                } else {
                    expr = ExpressionNode::MemberAccess(self.arena.alloc(expr), member);
                }
            }
            return Ok(expr);
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
                // Check if it's a generic invocation like `Test<int>(...)` or `Box<int> { ... }`,
                // tracking generic nesting so `Pair<Box<int>, int> { ... }` is recognized.
                if let Some(after) = self.scan_generic_args(2) {
                    match self.peek_token(after).kind {
                        TokenKind::OpenParenthesisToken => is_invocation = true,
                        TokenKind::CurlyOpenBracketToken => is_struct_instantiation = true,
                        _ => {}
                    }
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
                    generic_arguments = Some(self.parse_generic_args()?);
                }
                
                self.match_token(TokenKind::CurlyOpenBracketToken);
                let mut fields = Vec::new();
                while self.current_token().kind != TokenKind::CurlyCloseBracketToken && self.current_token().kind != TokenKind::EndOfFileToken {
                    let iter = self.current_token_index;
                    let field_name = self.match_token(TokenKind::IdentifierToken);
                    self.match_token(TokenKind::ColonToken);
                    let field_value = self.parse_expression(0)?;
                    fields.push((field_name, field_value));
                    if self.current_token().kind == TokenKind::CommaToken {
                        self.match_token(TokenKind::CommaToken);
                    }
                    self.ensure_progress(iter);
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
                            // Method generic args, e.g. `obj.cast<Foo<int>>()`. Only treat as
                            // generic when the balanced `<...>` is immediately followed by `(`.
                            let is_generic = self.scan_generic_args(1)
                                .map(|after| self.peek_token(after).kind == TokenKind::OpenParenthesisToken)
                                .unwrap_or(false);
                            if is_generic {
                                self.match_token(TokenKind::SmallerThanToken);
                                generic_args = Some(self.parse_generic_args()?);
                            }
                        }
                        
                        if self.current_token().kind == TokenKind::OpenParenthesisToken {
                            self.match_token(TokenKind::OpenParenthesisToken);
                            let mut params = Vec::new();
                            while self.current_token().kind != TokenKind::CloseParenthesisToken && self.current_token().kind != TokenKind::EndOfFileToken {
                                let iter = self.current_token_index;
                                params.push(self.parse_expression(0)?);
                                if self.current_token().kind == TokenKind::CommaToken {
                                    self.match_token(TokenKind::CommaToken);
                                }
                                self.ensure_progress(iter);
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
        else if self.current_token().kind==TokenKind::CharToken
        {
            // A char literal `'a'` is a `char` whose backing token text is the (ASCII/code point)
            // value, so codegen can emit `i32.const <value>`. Escapes like '\n', '\t', '\\', '\''
            // and '\0' are supported.
            let tok = self.next_token();
            let value = Self::char_literal_value(&tok.text);
            let char_token = SyntaxToken::new(TokenKind::CharToken, tok.position.clone(), value.to_string());
            return Ok(ExpressionNode::Literal(Type::Char(char_token)));
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
    pub(super) fn parse_invocation_expression(&mut self)->Result<ExpressionNode<'a>,Error>
    {
        let function_name=self.match_token(TokenKind::IdentifierToken);
        
        let mut generic_arguments = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            generic_arguments = Some(self.parse_generic_args()?);
        }

        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken);
        let mut arguments=Vec::new();
        while self.current_token().kind!=TokenKind::CloseParenthesisToken && self.current_token().kind!=EndOfFileToken
        {
            let iter = self.current_token_index;
            //parse the argument
            let argument=self.parse_expression(0)?;
            arguments.push(argument);
            if self.current_token().kind==TokenKind::CommaToken && self.peek_token(1).kind!=TokenKind::CloseParenthesisToken
            {
                //eat the comma
                self.match_token(TokenKind::CommaToken);
            }
            self.ensure_progress(iter);
        }
        //eat the close parenthesis
        self.match_token(TokenKind::CloseParenthesisToken);
        Ok(ExpressionNode::FunctionCall(function_name, generic_arguments, arguments))
    }
}
