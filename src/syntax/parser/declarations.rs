use std::collections::HashMap;
use std::io::Error;
use bumpalo::Bump;
use crate::syntax::nodes::{ExpressionNode, FunctionNode, Type, ParameterNode, ProgramNode, StatementNode, ImportNode};
use crate::syntax::syntax_tree::SyntaxTree;
use crate::syntax::text::line_text::LineText;
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::syntax::token::token_kind::TokenKind::{EndOfFileToken, IdentifierToken};
use crate::syntax::lexer::Lexer;
use crate::driver::diagnostics::DiagnosticBag;
use super::Parser;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses a type alias: `type Name = ExistingType;`. The alias is recorded and resolved
    /// (erased) during `parse_type`, so it must be declared before use.
    pub(super) fn parse_type_alias(&mut self) -> Result<(), Error> {
        self.match_token(TokenKind::TypeToken);
        let name = self.match_token(TokenKind::IdentifierToken);
        self.match_token(TokenKind::EqualToken);
        let aliased = self.parse_type()?;
        self.match_token(TokenKind::SemicolonToken);
        if self.type_aliases.contains_key(&name.text) {
            self.diagnostics.report_error(
                format!("Type alias '{}' is already defined", name.text),
                Some(name.position.clone()),
            );
        }
        self.type_aliases.insert(name.text, aliased);
        Ok(())
    }

    /// Parses an enum declaration: `enum Name { A, B = 5, C }`. Members without an explicit value
    /// continue from the previous member's value (starting at 0), C-style.
    pub(super) fn parse_enum_declaration(&mut self) -> Result<crate::syntax::nodes::EnumDeclarationNode, Error> {
        self.match_token(TokenKind::EnumToken);
        let name = self.match_token(TokenKind::IdentifierToken);
        self.match_token(TokenKind::CurlyOpenBracketToken);

        let mut members = Vec::new();
        let mut next_value: i32 = 0;
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let index_before = self.current_token_index;
            let member_name = self.match_token(TokenKind::IdentifierToken);
            let value = if self.current_token().kind == TokenKind::EqualToken {
                self.match_token(TokenKind::EqualToken);
                let num = self.match_token(TokenKind::NumberToken);
                num.text.parse::<i32>().unwrap_or(next_value)
            } else {
                next_value
            };
            next_value = value + 1;
            members.push((member_name, value));

            if self.current_token().kind == TokenKind::CommaToken {
                self.match_token(TokenKind::CommaToken);
            }
            // Safety: never spin on an unexpected token.
            if self.current_token_index == index_before {
                self.next_token();
            }
        }
        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(crate::syntax::nodes::EnumDeclarationNode::new(name, members))
    }
    
    /// Parses a struct declaration
    pub(super) fn parse_struct_declaration(&mut self) -> Result<crate::syntax::nodes::struct_node::StructDeclarationNode<'a>, Error> {
        let mut is_exported = false;
        if self.current_token().kind == TokenKind::PubToken {
            self.match_token(TokenKind::PubToken);
            is_exported = true;
        }
        
        self.match_token(TokenKind::StructToken);
        let struct_name = self.match_token(TokenKind::IdentifierToken);

        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                let iter = self.current_token_index;
                params.push(self.match_token(TokenKind::IdentifierToken));
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
            }
            self.match_token(TokenKind::GreaterThanToken);
            generic_parameters = Some(params);
        }

        self.match_token(TokenKind::CurlyOpenBracketToken);
        
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken && self.current_token().kind != TokenKind::EndOfFileToken {
            let iter = self.current_token_index;
            // `init(...)` / `drop(...)` without a leading `pub` still declare a constructor/
            // destructor method rather than a field.
            let is_ctor_dtor = self.current_token().kind == TokenKind::IdentifierToken
                && matches!(self.current_token().text.as_str(), "init" | "drop")
                && self.peek_token(1).kind == TokenKind::OpenParenthesisToken;
            if self.current_token().kind == TokenKind::FunToken || self.current_token().kind == TokenKind::PubToken || self.current_token().kind == TokenKind::AtToken || is_ctor_dtor {
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
                fields.push(crate::syntax::nodes::struct_node::StructFieldNode {
                    name: field_name,
                    type_token: field_type_token,
                });
            }
            self.ensure_progress(iter);
        }
        
        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(crate::syntax::nodes::struct_node::StructDeclarationNode::new(struct_name, generic_parameters, fields, methods, is_exported))
    }
    
    /// Parses an `extend Type { ... }` block: a set of methods attached to an existing type
    /// (a primitive, `object`, or a struct). The body holds method declarations only (no
    /// fields, no `init`/`drop`). The target name is normalized to its canonical primitive
    /// spelling (e.g. `String` -> `string`).
    pub(super) fn parse_extend_declaration(&mut self) -> Result<crate::syntax::nodes::ExtendNode<'a>, Error> {
        self.match_token(TokenKind::ExtendToken);

        let mut target = if self.current_token().kind == TokenKind::DataTypeToken {
            self.match_token(TokenKind::DataTypeToken)
        } else {
            self.match_token(TokenKind::IdentifierToken)
        };
        if let Some(canonical) = crate::syntax::nodes::types::canonical_type_name(&target.text) {
            target.text = canonical.to_string();
        }

        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                let iter = self.current_token_index;
                params.push(self.match_token(TokenKind::IdentifierToken));
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
            }
            self.match_token(TokenKind::GreaterThanToken);
            generic_parameters = Some(params);
        }

        self.match_token(TokenKind::CurlyOpenBracketToken);

        let mut methods = Vec::new();
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken && self.current_token().kind != TokenKind::EndOfFileToken {
            let iter = self.current_token_index;
            if self.current_token().kind == TokenKind::FunToken
                || self.current_token().kind == TokenKind::PubToken
                || self.current_token().kind == TokenKind::AtToken {
                methods.push(self.parse_function()?);
            } else {
                let cur = self.current_token();
                self.diagnostics.report_error(
                    format!("'extend' blocks may only contain methods, but found {:?}", cur.kind),
                    Some(cur.position.clone()),
                );
                self.next_token();
            }
            self.ensure_progress(iter);
        }

        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(crate::syntax::nodes::ExtendNode::new(target, generic_parameters, methods))
    }

    /// Parses an import statement
    pub(super) fn parse_import(&mut self)->Result<ImportNode,Error>
    {
        self.match_token(TokenKind::ImportToken);
        let module_name = self.match_token(TokenKind::StringToken);
        Ok(ImportNode::new(module_name))
    }
    /// Parses a Type from the token stream, including array types
    pub(super) fn parse_type(&mut self) -> Result<Type, Error> {
        // Function type: `fun(param, ...): ret` (the return annotation is optional, defaulting to
        // void). Used for first-class function values and function parameters.
        if self.current_token().kind == TokenKind::FunToken {
            self.match_token(TokenKind::FunToken);
            self.match_token(TokenKind::OpenParenthesisToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::CloseParenthesisToken
                && self.current_token().kind != TokenKind::EndOfFileToken
            {
                let iter = self.current_token_index;
                params.push(self.parse_type()?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
            }
            self.match_token(TokenKind::CloseParenthesisToken);
            let ret = if self.current_token().kind == TokenKind::ColonToken {
                self.match_token(TokenKind::ColonToken);
                self.parse_type()?
            } else {
                Type::Void
            };
            return Ok(Type::Function(params, Box::new(ret)));
        }

        let type_token = if self.current_token().kind == TokenKind::DataTypeToken {
            self.match_token(TokenKind::DataTypeToken)
        } else {
            self.match_token(TokenKind::IdentifierToken)
        };
        let mut parsed_type = Type::from_token(type_token)?;

        // Resolve a type alias to its underlying type (unless generic args follow). Array/nullable
        // suffixes below still apply to the resolved type.
        if let Type::Struct(token, None) = &parsed_type {
            if self.current_token().kind != TokenKind::SmallerThanToken {
                if let Some(alias) = self.type_aliases.get(&token.text) {
                    parsed_type = alias.clone();
                }
            }
        }
        
        // Check for generic arguments
        if let Type::Struct(token, _) = &parsed_type {
            if self.current_token().kind == TokenKind::SmallerThanToken {
                self.match_token(TokenKind::SmallerThanToken);
                let args = self.parse_generic_args()?;
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
    pub(super) fn parse_function(&mut self)->Result<FunctionNode<'a>,Error>
    {
        // Optional attributes precede `export`/`extern`/`fun`:
        //   `@override`           - object-protocol method override
        //   `@js("mod", "name")`  - remap an `extern` import's module/field name
        let mut is_override = false;
        let mut import_module: Option<String> = None;
        let mut import_name: Option<String> = None;
        while self.current_token().kind == TokenKind::AtToken {
            self.match_token(TokenKind::AtToken);
            let attr = self.match_token(TokenKind::IdentifierToken);
            if attr.text == "override" {
                is_override = true;
            } else if attr.text == "js" {
                let (module, name) = self.parse_js_attribute_args();
                import_module = module;
                import_name = name;
            } else {
                self.diagnostics.report_error(
                    format!("Unknown attribute '@{}'", attr.text),
                    Some(attr.position.clone())
                );
            }
        }

        let mut is_exported = false;
        if self.current_token().kind == TokenKind::PubToken {
            self.match_token(TokenKind::PubToken);
            is_exported = true;
        }

        let mut is_extern = false;
        if self.current_token().kind == TokenKind::ExternToken {
            self.match_token(TokenKind::ExternToken);
            is_extern = true;
            if is_exported {
                self.diagnostics.report_error(
                    "A function cannot be both 'pub' and 'extern'".to_string(),
                    Some(self.current_token().position.clone())
                );
            }
        }

        // Constructor (`init`) / destructor (`drop`) declarations omit the `fun` keyword and the
        // return type; they are lowered to ordinary methods named `init`/`drop` and dispatched
        // specially (constructor calls, scope-exit destructor calls).
        if self.current_token().kind == TokenKind::IdentifierToken
            && matches!(self.current_token().text.as_str(), "init" | "drop") {
            let ctor_name = self.match_token(TokenKind::IdentifierToken);
            let params = self.parse_formal_parameters()?;
            let block = self.parse_block()?;
            return Ok(FunctionNode::new(ctor_name, None, None, params, block, is_exported));
        }

        //eat the fun keyword
        self.match_token(TokenKind::FunToken);
        let function_name=self.match_token(TokenKind::IdentifierToken);
        
        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken && self.current_token().kind != TokenKind::EndOfFileToken {
                let iter = self.current_token_index;
                params.push(self.match_token(TokenKind::IdentifierToken));
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
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

        if is_extern {
            // Extern functions are lowered to WASM imports: no body, terminated by `;`.
            if generic_parameters.is_some() {
                self.diagnostics.report_error(
                    "Extern functions cannot be generic".to_string(),
                    Some(function_name.position.clone())
                );
            }
            self.match_token(TokenKind::SemicolonToken);
            let empty: &'a [StatementNode<'a>] = self.arena.alloc_slice_fill_iter(std::iter::empty());
            let mut node = FunctionNode::new(function_name.clone(), generic_parameters, return_type, params, empty, false);
            node.is_extern = true;
            node.import_module = import_module.or_else(|| Some("env".to_string()));
            node.import_name = import_name.or_else(|| Some(function_name.text.clone()));
            return Ok(node);
        }

        let block=self.parse_block()?;
        let mut node = FunctionNode::new(function_name,generic_parameters,return_type,params,block,is_exported);
        node.is_override = is_override;
        Ok(node)
    }

    /// Parses the arguments of a `@js(...)` attribute: `("module")` or `("module", "name")`.
    /// Returns `(module, name)`, each `None` if absent. String literals are unquoted.
    pub(super) fn parse_js_attribute_args(&mut self) -> (Option<String>, Option<String>) {
        let mut module = None;
        let mut name = None;
        if self.current_token().kind == TokenKind::OpenParenthesisToken {
            self.match_token(TokenKind::OpenParenthesisToken);
            if self.current_token().kind == TokenKind::StringToken {
                module = Some(self.match_token(TokenKind::StringToken).text.trim_matches('"').to_string());
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                    if self.current_token().kind == TokenKind::StringToken {
                        name = Some(self.match_token(TokenKind::StringToken).text.trim_matches('"').to_string());
                    }
                }
            }
            self.match_token(TokenKind::CloseParenthesisToken);
        }
        (module, name)
    }
    /// Parses formal parameters for a function declaration
    pub(super) fn parse_formal_parameters(&mut self)->Result<Vec<ParameterNode>,Error>
    {
        let mut params=vec![];
        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken);

        while self.current_token().kind != TokenKind::CloseParenthesisToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
           let index_before = self.current_token_index;
           //eat the identifier
           let param=self.match_token(TokenKind::IdentifierToken);
            //eat the colon
            self.match_token(TokenKind::ColonToken);
            
            let param_type = self.parse_type()?;
            params.push(ParameterNode::new(param, param_type));

            // Safety: if a malformed parameter consumed no tokens (e.g. a reserved keyword used
            // as a parameter name), advance one token to avoid an infinite loop.
            if self.current_token_index == index_before {
                self.next_token();
            }
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
}
