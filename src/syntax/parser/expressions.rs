use super::Parser;
use crate::syntax::lexer::Lexer;
use crate::syntax::nodes::{ExpressionNode, MatchArm, MatchArmBody, PatternNode, Type};
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::syntax::token::token_kind::TokenKind::{EndOfFileToken, IdentifierToken};
use std::io::Error;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses an expression with operator precedence
    pub(super) fn parse_expression(
        &mut self,
        parent_precedence: i32,
    ) -> Result<ExpressionNode<'a>, Error> {
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
        loop {
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
                left = ExpressionNode::Binary(
                    self.arena.alloc(left),
                    operator_token,
                    self.arena.alloc(right),
                );
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
    pub(super) fn parse_primary_expression(&mut self) -> Result<ExpressionNode<'a>, Error> {
        // `match (subject) { ... }` expression.
        if self.current_token().kind == TokenKind::MatchToken {
            return self.parse_match();
        }
        //parse parenthesized expressions or cast
        if self.current_token().kind == TokenKind::OpenParenthesisToken {
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
                    matches!(
                        next_kind,
                        TokenKind::NumberToken
                            | TokenKind::StringToken
                            | TokenKind::BooleanToken
                            | TokenKind::IdentifierToken
                            | TokenKind::OpenParenthesisToken
                            | TokenKind::OpenBracketToken
                            | TokenKind::MinusToken
                            | TokenKind::BangToken
                    )
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
                return Ok(ExpressionNode::Cast(
                    cast_type,
                    self.arena.alloc(expression),
                ));
            }

            //eat the open parenthesis
            self.match_token(TokenKind::OpenParenthesisToken);
            let expression = self.parse_expression(0)?;
            //eat the close parenthesis
            self.match_token(TokenKind::CloseParenthesisToken);
            // Allow postfix access on a parenthesized expression, e.g. `(7).hash_code()`,
            // `("x" + y).len()`, or `(arr)[0]`. This is required for method calls on literals
            // whose bare form would mis-lex (`7.hash_code()` reads `7.` as a float).
            let parenthesized = ExpressionNode::Parenthesized(self.arena.alloc(expression));
            return self.parse_postfix_chain(parenthesized);
        } else if self.current_token().kind == TokenKind::OpenBracketToken {
            // Array literal
            self.match_token(TokenKind::OpenBracketToken);
            let mut elements = Vec::new();
            while self.current_token().kind != TokenKind::CloseBracketToken
                && self.current_token().kind != TokenKind::EndOfFileToken
            {
                let iter = self.current_token_index;
                elements.push(self.parse_expression(0)?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
            }
            self.match_token(TokenKind::CloseBracketToken);
            return Ok(ExpressionNode::ArrayLiteral(elements));
        } else if self.current_token().kind == TokenKind::BooleanToken {
            return Ok(ExpressionNode::Literal(Type::Boolean(
                self.match_token(TokenKind::BooleanToken),
            )));
        } else if self.current_token().kind == TokenKind::NullToken {
            self.match_token(TokenKind::NullToken);
            // `Nullable(Void)` represents the `null` literal until its concrete type is known.
            return Ok(ExpressionNode::Literal(Type::Nullable(Box::new(
                Type::Void,
            ))));
        }
        // A primitive type name used as a static-call receiver, e.g. `int.parse("5")`. The
        // keyword is treated as an identifier so the member/method-access loop below applies;
        // static dispatch is resolved later by the analyzer/codegen.
        else if self.current_token().kind == TokenKind::DataTypeToken
            && self.peek_token(1).kind == TokenKind::DotToken
        {
            let mut expr = ExpressionNode::Identifier(self.next_token());
            while self.current_token().kind == TokenKind::DotToken {
                self.match_token(TokenKind::DotToken);
                let member = self.match_name_token();
                let mut generic_args = None;
                if self.current_token().kind == TokenKind::SmallerThanToken {
                    let is_generic = self
                        .scan_generic_args(1)
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
                    while self.current_token().kind != TokenKind::CloseParenthesisToken
                        && self.current_token().kind != TokenKind::EndOfFileToken
                    {
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
                        params,
                    );
                } else {
                    expr = ExpressionNode::MemberAccess(self.arena.alloc(expr), member);
                }
            }
            return Ok(expr);
        }
        //parse identifiers
        else if self.current_token().kind == IdentifierToken {
            let mut is_invocation = false;

            if self.peek_token(1).kind == TokenKind::OpenParenthesisToken {
                is_invocation = true;
            } else if self.peek_token(1).kind == TokenKind::SmallerThanToken {
                // Generic invocation like `Test<int>(...)`, tracking generic nesting so
                // `make<Pair<Box<int>, int>>(...)` is recognized as a call.
                if let Some(after) = self.scan_generic_args(2) {
                    if self.peek_token(after).kind == TokenKind::OpenParenthesisToken {
                        is_invocation = true;
                    }
                }
            }

            if is_invocation {
                // A call on a bare identifier (free function or constructor, e.g.
                // `HttpClient(url)`) can still be the base of a postfix chain like
                // `HttpClient(url).set_header(...)` or `make().field`.
                let expr = self.parse_invocation_expression()?;
                return self.parse_postfix_chain(expr);
            } else {
                let mut expr = ExpressionNode::Identifier(self.next_token());

                // Check for index access or member access
                loop {
                    if self.current_token().kind == TokenKind::OpenBracketToken {
                        self.match_token(TokenKind::OpenBracketToken);
                        let index = self.parse_expression(0)?;
                        self.match_token(TokenKind::CloseBracketToken);
                        expr = ExpressionNode::IndexAccess(
                            self.arena.alloc(expr),
                            self.arena.alloc(index),
                        );
                    } else if self.current_token().kind == TokenKind::DotToken {
                        self.match_token(TokenKind::DotToken);
                        let member = self.match_name_token();

                        let mut generic_args = None;
                        if self.current_token().kind == TokenKind::SmallerThanToken {
                            // Method generic args, e.g. `obj.cast<Foo<int>>()`. Only treat as
                            // generic when the balanced `<...>` is immediately followed by `(`.
                            let is_generic = self
                                .scan_generic_args(1)
                                .map(|after| {
                                    self.peek_token(after).kind == TokenKind::OpenParenthesisToken
                                })
                                .unwrap_or(false);
                            if is_generic {
                                self.match_token(TokenKind::SmallerThanToken);
                                generic_args = Some(self.parse_generic_args()?);
                            }
                        }

                        if self.current_token().kind == TokenKind::OpenParenthesisToken {
                            self.match_token(TokenKind::OpenParenthesisToken);
                            let mut params = Vec::new();
                            while self.current_token().kind != TokenKind::CloseParenthesisToken
                                && self.current_token().kind != TokenKind::EndOfFileToken
                            {
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
                                params,
                            );
                        } else {
                            expr = ExpressionNode::MemberAccess(self.arena.alloc(expr), member);
                        }
                    } else {
                        break;
                    }
                }

                return Ok(expr);
            }
        } else if self.current_token().kind == TokenKind::NumberToken {
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
            } else {
                return Ok(ExpressionNode::Literal(Type::Integer(self.next_token())));
            }
        } else if self.current_token().kind == TokenKind::StringToken {
            return Ok(ExpressionNode::Literal(Type::String(self.next_token())));
        } else if self.current_token().kind == TokenKind::InterpolatedStringToken {
            let tok = self.next_token();
            return self.parse_interpolated_string(tok);
        } else if self.current_token().kind == TokenKind::CharToken {
            // A char literal `'a'` is a `char` whose backing token text is the (ASCII/code point)
            // value, so codegen can emit `i32.const <value>`. Escapes like '\n', '\t', '\\', '\''
            // and '\0' are supported.
            let tok = self.next_token();
            let value = Self::char_literal_value(&tok.text);
            let char_token =
                SyntaxToken::new(TokenKind::CharToken, tok.position, value.to_string());
            return Ok(ExpressionNode::Literal(Type::Char(char_token)));
        }

        let cur = self.current_token();
        if cur.kind != TokenKind::IdentifierToken {
            self.diagnostics.report_error(
                format!("Expected expression but found {:?}", cur.kind),
                Some(cur.position),
            );
            self.next_token(); // skip the unexpected token to avoid infinite loop
            return Ok(ExpressionNode::Identifier(SyntaxToken::new(
                TokenKind::IdentifierToken,
                cur.position,
                "".to_string(),
            )));
        }

        let identifier = self.match_token(TokenKind::IdentifierToken);
        Ok(ExpressionNode::Identifier(identifier))
    }
    /// Continues parsing index (`[...]`) and member/method (`.name` / `.name(...)`) accesses onto an
    /// already-parsed base expression. Used so a call on a bare identifier (e.g. a constructor like
    /// `HttpClient(url)`) can be chained: `HttpClient(url).set_header(...)`.
    pub(super) fn parse_postfix_chain(
        &mut self,
        base: ExpressionNode<'a>,
    ) -> Result<ExpressionNode<'a>, Error> {
        let mut expr = base;
        loop {
            if self.current_token().kind == TokenKind::OpenBracketToken {
                self.match_token(TokenKind::OpenBracketToken);
                let index = self.parse_expression(0)?;
                self.match_token(TokenKind::CloseBracketToken);
                expr = ExpressionNode::IndexAccess(self.arena.alloc(expr), self.arena.alloc(index));
            } else if self.current_token().kind == TokenKind::DotToken {
                self.match_token(TokenKind::DotToken);
                let member = self.match_name_token();

                let mut generic_args = None;
                if self.current_token().kind == TokenKind::SmallerThanToken {
                    let is_generic = self
                        .scan_generic_args(1)
                        .map(|after| {
                            self.peek_token(after).kind == TokenKind::OpenParenthesisToken
                        })
                        .unwrap_or(false);
                    if is_generic {
                        self.match_token(TokenKind::SmallerThanToken);
                        generic_args = Some(self.parse_generic_args()?);
                    }
                }

                if self.current_token().kind == TokenKind::OpenParenthesisToken {
                    self.match_token(TokenKind::OpenParenthesisToken);
                    let mut params = Vec::new();
                    while self.current_token().kind != TokenKind::CloseParenthesisToken
                        && self.current_token().kind != TokenKind::EndOfFileToken
                    {
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
                        params,
                    );
                } else {
                    expr = ExpressionNode::MemberAccess(self.arena.alloc(expr), member);
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }

    /// Parses a function invocation expression
    pub(super) fn parse_invocation_expression(&mut self) -> Result<ExpressionNode<'a>, Error> {
        let function_name = self.match_token(TokenKind::IdentifierToken);

        let mut generic_arguments = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            generic_arguments = Some(self.parse_generic_args()?);
        }

        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken);
        let mut arguments = Vec::new();
        while self.current_token().kind != TokenKind::CloseParenthesisToken
            && self.current_token().kind != EndOfFileToken
        {
            let iter = self.current_token_index;
            //parse the argument
            let argument = self.parse_expression(0)?;
            arguments.push(argument);
            if self.current_token().kind == TokenKind::CommaToken
                && self.peek_token(1).kind != TokenKind::CloseParenthesisToken
            {
                //eat the comma
                self.match_token(TokenKind::CommaToken);
            }
            self.ensure_progress(iter);
        }
        //eat the close parenthesis
        self.match_token(TokenKind::CloseParenthesisToken);
        Ok(ExpressionNode::FunctionCall(
            function_name,
            generic_arguments,
            arguments,
        ))
    }

    /// Parses a `match (subject) { pattern [if guard] => body, ... }` expression. Each arm body is
    /// either an expression (`=> expr`) or a statement block (`=> { ... }`); a trailing comma after
    /// an arm is optional.
    pub(super) fn parse_match(&mut self) -> Result<ExpressionNode<'a>, Error> {
        self.match_token(TokenKind::MatchToken);
        self.match_token(TokenKind::OpenParenthesisToken);
        let subject = self.parse_expression(0)?;
        self.match_token(TokenKind::CloseParenthesisToken);
        self.match_token(TokenKind::CurlyOpenBracketToken);

        let mut arms = Vec::new();
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != EndOfFileToken
        {
            let iter = self.current_token_index;
            let pattern = self.parse_pattern()?;

            // Optional `if <guard>` after the pattern.
            let guard = if self.current_token().kind == TokenKind::IfToken {
                self.match_token(TokenKind::IfToken);
                Some(self.parse_expression(0)?)
            } else {
                None
            };

            self.match_token(TokenKind::FatArrowToken);

            let body = if self.current_token().kind == TokenKind::CurlyOpenBracketToken {
                MatchArmBody::Block(self.parse_block()?)
            } else {
                MatchArmBody::Expr(self.parse_expression(0)?)
            };

            // A trailing comma between arms is optional.
            if self.current_token().kind == TokenKind::CommaToken {
                self.match_token(TokenKind::CommaToken);
            }

            arms.push(MatchArm {
                pattern,
                guard,
                body,
            });
            self.ensure_progress(iter);
        }
        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(ExpressionNode::Match(self.arena.alloc(subject), arms))
    }

    /// Parses a single match pattern: `_` (wildcard), a literal, a bare identifier (a binding,
    /// later reinterpreted as a unit variant by the analyzer when it names one), or a variant
    /// pattern `Variant(sub, ...)` / `Enum.Variant(sub, ...)`.
    pub(super) fn parse_pattern(&mut self) -> Result<PatternNode, Error> {
        let cur = self.current_token();
        match cur.kind {
            TokenKind::IdentifierToken => {
                if cur.text == "_" {
                    let tok = self.next_token();
                    return Ok(PatternNode::Wildcard(tok));
                }
                let first = self.match_token(TokenKind::IdentifierToken);
                // `Enum.Variant[(...)]` - a qualified variant pattern.
                if self.current_token().kind == TokenKind::DotToken {
                    self.match_token(TokenKind::DotToken);
                    let variant = self.match_token(TokenKind::IdentifierToken);
                    let subs = self.parse_pattern_args()?;
                    return Ok(PatternNode::Variant(Some(first), variant, subs));
                }
                // `Variant(...)` - an unqualified variant pattern with a payload.
                if self.current_token().kind == TokenKind::OpenParenthesisToken {
                    let subs = self.parse_pattern_args()?;
                    return Ok(PatternNode::Variant(None, first, subs));
                }
                // A bare identifier: a binding (or a unit variant, resolved during analysis).
                Ok(PatternNode::Binding(first))
            }
            _ => Ok(PatternNode::Literal(self.parse_literal_pattern()?)),
        }
    }

    /// Parses the parenthesized sub-pattern list of a variant pattern, e.g. the `(x, None)` in
    /// `Pair(x, None)`. Returns an empty list when there is no `(...)`.
    fn parse_pattern_args(&mut self) -> Result<Vec<PatternNode>, Error> {
        let mut subs = Vec::new();
        if self.current_token().kind == TokenKind::OpenParenthesisToken {
            self.match_token(TokenKind::OpenParenthesisToken);
            while self.current_token().kind != TokenKind::CloseParenthesisToken
                && self.current_token().kind != EndOfFileToken
            {
                let iter = self.current_token_index;
                subs.push(self.parse_pattern()?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
            }
            self.match_token(TokenKind::CloseParenthesisToken);
        }
        Ok(subs)
    }

    /// Parses a literal used as a pattern (`0`, `-5`, `3.14`, `"s"`, `'c'`, `true`, `null`).
    fn parse_literal_pattern(&mut self) -> Result<Type, Error> {
        let cur = self.current_token();
        match cur.kind {
            TokenKind::BooleanToken => Ok(Type::Boolean(self.match_token(TokenKind::BooleanToken))),
            TokenKind::StringToken => Ok(Type::String(self.match_token(TokenKind::StringToken))),
            TokenKind::NullToken => {
                self.match_token(TokenKind::NullToken);
                Ok(Type::Nullable(Box::new(Type::Void)))
            }
            TokenKind::CharToken => {
                let tok = self.next_token();
                let value = Self::char_literal_value(&tok.text);
                let char_token =
                    SyntaxToken::new(TokenKind::CharToken, tok.position, value.to_string());
                Ok(Type::Char(char_token))
            }
            TokenKind::MinusToken | TokenKind::NumberToken => {
                let negative = cur.kind == TokenKind::MinusToken;
                if negative {
                    self.match_token(TokenKind::MinusToken);
                }
                let mut token = self.match_token(TokenKind::NumberToken);
                let mut text = token.text.clone();
                let is_double = text.ends_with('d') || text.ends_with('D');
                let is_float = text.ends_with('f') || text.ends_with('F');
                if is_double || is_float {
                    text = text[..text.len() - 1].to_string();
                }
                if negative {
                    text = format!("-{}", text);
                }
                token.text = text.clone();
                if is_double {
                    Ok(Type::Double(token))
                } else if is_float {
                    Ok(Type::Float(token))
                } else if text.contains('.') {
                    Ok(Type::Float(token))
                } else {
                    Ok(Type::Integer(token))
                }
            }
            _ => {
                self.diagnostics.report_error(
                    format!("Expected a pattern but found {}", cur.kind.friendly_name()),
                    Some(cur.position),
                );
                self.next_token();
                Ok(Type::Unknown)
            }
        }
    }

    /// Lowers an interpolated string literal `$"...{expr}..."` into the existing string
    /// concatenation chain. `$"{y+68} is {x}"` becomes `"" + (y + 68) + " is " + (x)`, reusing
    /// the analyzer/codegen `string + T` path that auto-converts each non-string operand through
    /// the `to_string` object protocol. The chain is seeded with an empty string literal so the
    /// whole expression is always typed `string`, even for a lone hole like `$"{x}"`.
    fn parse_interpolated_string(
        &mut self,
        token: SyntaxToken,
    ) -> Result<ExpressionNode<'a>, Error> {
        let pos = token.position;
        // Strip the leading `$"` and trailing `"`. The lexer guarantees this shape.
        let raw = token.text.as_str();
        let body = raw
            .strip_prefix("$\"")
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or("");

        // Byte offset (in the original file) of the first character of `body`: skip `$"`.
        let body_base = pos.start + 2;

        let chars: Vec<char> = body.chars().collect();
        let mut i = 0;
        // Byte offset of `chars[i]` within `body`, kept in lockstep with `i` so hole sources can be
        // mapped back to absolute file positions for IDE navigation.
        let mut byte_pos = 0usize;
        let mut text_buf = String::new();
        // Each segment is either literal text (`Ok`) or a hole `(source, byte offset in body)`.
        let mut segments: Vec<Result<String, (String, usize)>> = Vec::new();

        while i < chars.len() {
            let c = chars[i];
            if c == '{' {
                // `{{` is an escaped literal `{`.
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    text_buf.push('{');
                    byte_pos += 2;
                    i += 2;
                    continue;
                }
                // Open a hole: flush any pending literal text first.
                if !text_buf.is_empty() {
                    segments.push(Ok(std::mem::take(&mut text_buf)));
                }
                byte_pos += 1; // consume `{`
                i += 1;
                let hole_byte_start = byte_pos;
                let mut depth = 1;
                let mut hole = String::new();
                while i < chars.len() && depth > 0 {
                    let h = chars[i];
                    let advance = h.len_utf8();
                    if h == '{' {
                        depth += 1;
                        hole.push(h);
                    } else if h == '}' {
                        depth -= 1;
                        if depth == 0 {
                            byte_pos += advance; // consume the matching `}`
                            i += 1;
                            break;
                        }
                        hole.push(h);
                    } else {
                        hole.push(h);
                    }
                    byte_pos += advance;
                    i += 1;
                }
                if depth > 0 {
                    self.diagnostics.report_error(
                        "unterminated '{' in interpolated string".to_string(),
                        Some(pos),
                    );
                }
                segments.push(Err((hole, hole_byte_start)));
            } else if c == '}' {
                // `}}` is an escaped literal `}`.
                if i + 1 < chars.len() && chars[i + 1] == '}' {
                    text_buf.push('}');
                    byte_pos += 2;
                    i += 2;
                    continue;
                }
                self.diagnostics.report_error(
                    "unmatched '}' in interpolated string; use '}}' for a literal brace"
                        .to_string(),
                    Some(pos),
                );
                text_buf.push('}');
                byte_pos += 1;
                i += 1;
            } else {
                text_buf.push(c);
                byte_pos += c.len_utf8();
                i += 1;
            }
        }
        if !text_buf.is_empty() {
            segments.push(Ok(text_buf));
        }

        // Seed with an empty string literal so the result is always `string`.
        let mut acc = self.make_string_literal(String::new(), pos);
        for segment in segments {
            let right = match segment {
                Ok(text) => self.make_string_literal(text, pos),
                Err((hole, hole_byte_start)) => {
                    self.parse_interpolation_hole(hole, body_base + hole_byte_start, pos)?
                }
            };
            let left_ref = self.arena.alloc(acc);
            let right_ref = self.arena.alloc(right);
            let plus = SyntaxToken::new(TokenKind::PlusToken, pos, "+".to_string());
            acc = ExpressionNode::Binary(left_ref, plus, right_ref);
        }
        Ok(acc)
    }

    /// Builds a string literal AST node from raw (already-escaped) inner text by re-adding the
    /// surrounding quotes that codegen strips. Any backslash escapes carried over from the
    /// interpolated literal are preserved verbatim, matching plain string literals.
    fn make_string_literal(
        &self,
        text: String,
        pos: crate::syntax::text::text_span::TextSpan,
    ) -> ExpressionNode<'a> {
        let tok = SyntaxToken::new(TokenKind::StringToken, pos, format!("\"{}\"", text));
        ExpressionNode::Literal(Type::String(tok))
    }

    /// Parses the source of a single `{...}` hole into an expression using a child parser that
    /// shares this parser's arena and diagnostics (so allocated nodes live in the same arena and
    /// errors surface on the same bag). `abs_offset` is the byte position of the hole's first
    /// character in the original file; sub-token spans are remapped to absolute file coordinates so
    /// IDE features (hover, go-to-definition, references) resolve correctly inside `{holes}`.
    fn parse_interpolation_hole(
        &mut self,
        source: String,
        abs_offset: usize,
        pos: crate::syntax::text::text_span::TextSpan,
    ) -> Result<ExpressionNode<'a>, Error> {
        if source.trim().is_empty() {
            self.diagnostics.report_error(
                "empty '{}' interpolation hole in string".to_string(),
                Some(pos),
            );
            return Ok(self.make_string_literal(String::new(), pos));
        }

        let parent_line_text = self.lexer.line_text();
        let mut lexer = Lexer::new(source);
        let mut tokens = lexer.lex_all(self.diagnostics);
        // Translate hole-relative byte spans to absolute file positions.
        for token in tokens.iter_mut() {
            token.position = crate::syntax::text::text_span::TextSpan::new(
                (
                    abs_offset + token.position.start,
                    abs_offset + token.position.end,
                ),
                &parent_line_text,
            );
        }
        let mut sub: Parser<'a, '_> = Parser {
            lexer,
            tokens,
            current_token_index: 0,
            arena: self.arena,
            diagnostics: &mut *self.diagnostics,
            foreach_counter: 0,
            type_aliases: self.type_aliases.clone(),
        };
        let expr = sub.parse_expression(0)?;
        if sub.current_token().kind != EndOfFileToken {
            let extra = sub.current_token();
            sub.diagnostics.report_error(
                format!(
                    "unexpected {} after expression in interpolation hole",
                    extra.kind.friendly_name()
                ),
                Some(pos),
            );
        }
        Ok(expr)
    }
}
