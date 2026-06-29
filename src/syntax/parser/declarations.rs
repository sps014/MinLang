use super::Parser;
use crate::syntax::nodes::{FunctionNode, ImportNode, ParameterNode, StatementNode, Type};
use crate::syntax::token::token_kind::TokenKind;
use std::io::Error;

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
                Some(name.position),
            );
        }
        self.type_aliases.insert(name.text, aliased);
        Ok(())
    }

    /// Parses an enum declaration: `enum Name { A, B = 5, C }`. Members without an explicit value
    /// continue from the previous member's value (starting at 0), C-style.
    pub(super) fn parse_enum_declaration(
        &mut self,
    ) -> Result<crate::syntax::nodes::EnumDeclarationNode, Error> {
        let first_trivia = self.current_token().leading_trivia.clone();
        self.match_token(TokenKind::EnumToken);
        let mut name = self.match_token(TokenKind::IdentifierToken);
        if !first_trivia.is_empty() {
            name.leading_trivia.splice(0..0, first_trivia);
        }
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
        Ok(crate::syntax::nodes::EnumDeclarationNode::new(
            name, members,
        ))
    }

    /// Parses a struct declaration
    pub(super) fn parse_struct_declaration(
        &mut self,
    ) -> Result<crate::syntax::nodes::struct_node::StructDeclarationNode<'a>, Error> {
        let first_trivia = self.current_token().leading_trivia.clone();

        let attributes = self.parse_attributes();

        let mut is_public = false;
        if self.current_token().kind == TokenKind::PublicToken {
            self.match_token(TokenKind::PublicToken);
            is_public = true;
        }

        self.match_token(TokenKind::ClassToken);
        let mut struct_name = self.match_token(TokenKind::IdentifierToken);
        if !first_trivia.is_empty() {
            struct_name.leading_trivia.splice(0..0, first_trivia);
        }

        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken
                && self.current_token().kind != TokenKind::EndOfFileToken
            {
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
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let iter = self.current_token_index;
            let field_attributes = self.parse_attributes();

            // Classify the member by looking past any leading `public`/`static`/`async`: a
            // method (`fun`, `static fun`, `constructor`/`del`, `extern fun`) is dispatched to
            // `parse_function` (which consumes its own modifiers), otherwise it is a field.
            let mut m = 0;
            while matches!(
                self.peek_token(m).kind,
                TokenKind::PublicToken | TokenKind::StaticToken | TokenKind::AsyncToken
            ) {
                m += 1;
            }
            let core = self.peek_token(m);
            let is_ctor_dtor = core.kind == TokenKind::IdentifierToken
                && crate::syntax::nodes::types::is_special_member_name(&core.text)
                && self.peek_token(m + 1).kind == TokenKind::OpenParenthesisToken;
            if core.kind == TokenKind::FunToken
                || core.kind == TokenKind::ExternToken
                || is_ctor_dtor
            {
                methods.push(self.parse_function(Some(field_attributes))?);
            } else {
                // Fields are private by default; an explicit `public` exposes them.
                let mut field_public = false;
                if self.current_token().kind == TokenKind::PublicToken {
                    self.match_token(TokenKind::PublicToken);
                    field_public = true;
                }
                let field_name = self.match_token(TokenKind::IdentifierToken);
                self.match_token(TokenKind::ColonToken);

                // Parse the full type (supporting generic args like `Map<string, JsonValue>`,
                // arrays, and nullable suffixes) and store its canonical spelling on the field.
                let type_position = self.current_token().position;
                let parsed_type = self.parse_type()?;
                let field_type_token = crate::syntax::token::syntax_token::SyntaxToken::new(
                    TokenKind::IdentifierToken,
                    type_position,
                    parsed_type.get_type(),
                );

                self.match_token(TokenKind::SemicolonToken);
                fields.push(crate::syntax::nodes::struct_node::StructFieldNode {
                    attributes: field_attributes,
                    name: field_name,
                    is_public: field_public,
                    type_token: field_type_token,
                    field_type: parsed_type,
                });
            }
            self.ensure_progress(iter);
        }

        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(
            crate::syntax::nodes::struct_node::StructDeclarationNode::new(
                attributes,
                struct_name,
                generic_parameters,
                fields,
                methods,
                is_public,
            ),
        )
    }

    /// Parses an `extend Type { ... }` block: a set of methods attached to an existing type
    /// (a primitive, `object`, or a struct). The body holds method declarations only (no
    /// fields, no `constructor`/`del`). The target name is normalized to its canonical primitive
    /// spelling (e.g. `String` -> `string`).
    pub(super) fn parse_extend_declaration(
        &mut self,
    ) -> Result<crate::syntax::nodes::ExtendNode<'a>, Error> {
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
            while self.current_token().kind != TokenKind::GreaterThanToken
                && self.current_token().kind != TokenKind::EndOfFileToken
            {
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
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let iter = self.current_token_index;
            let field_attributes = self.parse_attributes();
            if self.current_token().kind == TokenKind::FunToken
                || self.current_token().kind == TokenKind::PublicToken
                || self.current_token().kind == TokenKind::StaticToken
                || self.current_token().kind == TokenKind::AsyncToken
            {
                methods.push(self.parse_function(Some(field_attributes))?);
            } else {
                let cur = self.current_token();
                self.diagnostics.report_error(
                    format!(
                        "'extend' blocks may only contain methods, but found {:?}",
                        cur.kind
                    ),
                    Some(cur.position),
                );
                self.next_token();
            }
            self.ensure_progress(iter);
        }

        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(crate::syntax::nodes::ExtendNode::new(
            target,
            generic_parameters,
            methods,
        ))
    }

    /// Parses an import statement
    pub(super) fn parse_import(&mut self) -> Result<ImportNode, Error> {
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

    pub(super) fn parse_attributes(&mut self) -> Vec<crate::syntax::nodes::AttributeNode> {
        let mut attributes = Vec::new();
        while self.current_token().kind == TokenKind::AtToken {
            let at = self.match_token(TokenKind::AtToken);
            let mut name = self.match_token(TokenKind::IdentifierToken);
            // A doc comment preceding the declaration attaches to the `@` token (the first token of
            // the declaration). Thread it onto the attribute name so tooling can recover it even
            // when the attribute is parsed before the `fun`/`class` keyword.
            if !at.leading_trivia.is_empty() {
                name.leading_trivia.splice(0..0, at.leading_trivia);
            }
            let mut args = Vec::new();
            if self.current_token().kind == TokenKind::OpenParenthesisToken {
                self.match_token(TokenKind::OpenParenthesisToken);
                while self.current_token().kind != TokenKind::CloseParenthesisToken
                    && self.current_token().kind != TokenKind::EndOfFileToken
                {
                    let iter = self.current_token_index;
                    args.push(self.current_token().clone());
                    self.next_token();
                    if self.current_token().kind == TokenKind::CommaToken {
                        self.match_token(TokenKind::CommaToken);
                    }
                    self.ensure_progress(iter);
                }
                self.match_token(TokenKind::CloseParenthesisToken);
            }
            attributes.push(crate::syntax::nodes::AttributeNode { name, args });
        }
        attributes
    }

    /// Parses a function declaration
    pub(super) fn parse_function(
        &mut self,
        pre_parsed_attributes: Option<Vec<crate::syntax::nodes::AttributeNode>>,
    ) -> Result<FunctionNode<'a>, Error> {
        let mut first_trivia = self.current_token().leading_trivia.clone();

        let attributes = pre_parsed_attributes.unwrap_or_else(|| self.parse_attributes());

        // When attributes were parsed by the caller (e.g. struct members), the doc comment that
        // preceded the first attribute was consumed with it. Recover it from the attribute so the
        // comment still reaches the function name token below. (Whitespace is not trivia, so an
        // empty `first_trivia` reliably means "nothing but the attribute came before us".)
        if first_trivia.is_empty() {
            if let Some(first_attr) = attributes.first() {
                if !first_attr.name.leading_trivia.is_empty() {
                    first_trivia = first_attr.name.leading_trivia.clone();
                }
            }
        }

        // `async` may appear before or after `public` (e.g. `async fun`, `public async fun`,
        // `async public fun`). Calling such a function eagerly starts a task and yields `Future<T>`.
        let mut is_async = false;
        if self.current_token().kind == TokenKind::AsyncToken {
            self.match_token(TokenKind::AsyncToken);
            is_async = true;
        }

        let mut is_public = false;
        if self.current_token().kind == TokenKind::PublicToken {
            self.match_token(TokenKind::PublicToken);
            is_public = true;
        }

        if self.current_token().kind == TokenKind::AsyncToken {
            self.match_token(TokenKind::AsyncToken);
            is_async = true;
        }

        // `static fun ...`: a method with no implicit `this`, called as `Type.method(...)`.
        let mut is_static = false;
        if self.current_token().kind == TokenKind::StaticToken {
            self.match_token(TokenKind::StaticToken);
            is_static = true;
        }

        let mut is_extern = false;
        if self.current_token().kind == TokenKind::ExternToken {
            self.match_token(TokenKind::ExternToken);
            is_extern = true;
            if is_public {
                self.diagnostics.report_error(
                    "A function cannot be both 'public' and 'extern'".to_string(),
                    Some(self.current_token().position),
                );
            }
        }

        // allow `static` again in case order was reversed
        if self.current_token().kind == TokenKind::StaticToken {
            self.match_token(TokenKind::StaticToken);
            is_static = true;
        }

        // `static async fun ...`: allow `async` to follow `static` as well as precede it.
        if self.current_token().kind == TokenKind::AsyncToken {
            self.match_token(TokenKind::AsyncToken);
            is_async = true;
        }

        // Constructor (`constructor`) / destructor (`del`) declarations omit the `fun` keyword and
        // the return type; they are lowered to ordinary methods named `constructor`/`del` and
        // dispatched specially (constructor calls, scope-exit destructor calls). They cannot be
        // marked `public`.
        if self.current_token().kind == TokenKind::IdentifierToken
            && crate::syntax::nodes::types::is_special_member_name(&self.current_token().text)
        {
            let ctor_name = self.match_token(TokenKind::IdentifierToken);
            if is_public {
                self.diagnostics.report_error(
                    format!("'{}' cannot be marked 'public'", ctor_name.text),
                    Some(ctor_name.position),
                );
            }
            let params = self.parse_formal_parameters()?;
            let block = self.parse_block()?;
            return Ok(FunctionNode::new(
                attributes, ctor_name, None, None, params, block, false,
            ));
        }

        //eat the fun keyword
        self.match_token(TokenKind::FunToken);
        let mut function_name = self.match_token(TokenKind::IdentifierToken);
        if !first_trivia.is_empty() {
            function_name.leading_trivia.splice(0..0, first_trivia);
        }

        let mut generic_parameters = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut params = Vec::new();
            while self.current_token().kind != TokenKind::GreaterThanToken
                && self.current_token().kind != TokenKind::EndOfFileToken
            {
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

        let params = self.parse_formal_parameters()?;
        let mut return_type: Option<Type> = None;
        if self.current_token().kind == TokenKind::ColonToken {
            //eat the colon
            self.match_token(TokenKind::ColonToken);
            return_type = Some(self.parse_type()?);
        }

        if is_extern {
            // Extern functions are lowered to WASM imports: no body, terminated by `;`.
            let is_intrinsic = crate::intrinsics::has_intrinsic_attr(&attributes);
            if generic_parameters.is_some() && !is_intrinsic {
                self.diagnostics.report_error(
                    "Extern functions cannot be generic unless they are marked @intrinsic"
                        .to_string(),
                    Some(function_name.position),
                );
            }
            self.match_token(TokenKind::SemicolonToken);
            let empty: &'a [StatementNode<'a>] =
                self.arena.alloc_slice_fill_iter(std::iter::empty());
            let mut node = FunctionNode::new(
                attributes,
                function_name.clone(),
                generic_parameters,
                return_type,
                params,
                empty,
                false,
            );
            node.is_extern = true;
            node.is_static = is_static;
            node.is_async = is_async;
            return Ok(node);
        }

        let block = self.parse_block()?;
        let mut node = FunctionNode::new(
            attributes,
            function_name,
            generic_parameters,
            return_type,
            params,
            block,
            is_public,
        );
        node.is_static = is_static;
        node.is_async = is_async;
        Ok(node)
    }

    /// Parses a top-level variable declaration: an optional `public`/`static` modifier pair,
    /// then `let`/`const`, a name, an optional `: type` annotation, a required initializer, and a
    /// terminating `;`. Returns the assembled [`GlobalVariableNode`].
    pub(super) fn parse_global_variable(
        &mut self,
    ) -> Result<crate::syntax::nodes::GlobalVariableNode<'a>, Error> {
        let first_trivia = self.current_token().leading_trivia.clone();

        // `public` and `static` may appear in either order before `let`/`const`.
        let mut is_public = false;
        let mut is_static = false;
        loop {
            match self.current_token().kind {
                TokenKind::PublicToken => {
                    self.match_token(TokenKind::PublicToken);
                    is_public = true;
                }
                TokenKind::StaticToken => {
                    self.match_token(TokenKind::StaticToken);
                    is_static = true;
                }
                _ => break,
            }
        }

        let is_const = self.current_token().kind == TokenKind::ConstToken;
        if is_const {
            self.match_token(TokenKind::ConstToken);
        } else {
            self.match_token(TokenKind::LetToken);
        }

        let mut name = self.match_token(TokenKind::IdentifierToken);
        if !first_trivia.is_empty() {
            name.leading_trivia.splice(0..0, first_trivia);
        }

        let declared_type = if self.current_token().kind == TokenKind::ColonToken {
            self.match_token(TokenKind::ColonToken);
            Some(self.parse_type()?)
        } else {
            None
        };

        self.match_token(TokenKind::EqualToken);
        let initializer = self.parse_expression(0)?;
        self.match_token(TokenKind::SemicolonToken);

        Ok(crate::syntax::nodes::GlobalVariableNode {
            name,
            declared_type,
            initializer,
            is_const,
            is_public,
            is_static,
            file_path: None,
        })
    }

    /// Parses formal parameters for a function declaration
    pub(super) fn parse_formal_parameters(&mut self) -> Result<Vec<ParameterNode>, Error> {
        let mut params = vec![];
        //eat the open parenthesis
        self.match_token(TokenKind::OpenParenthesisToken);

        while self.current_token().kind != TokenKind::CloseParenthesisToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let index_before = self.current_token_index;
            //eat the identifier
            let param = self.match_token(TokenKind::IdentifierToken);
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
            if self.current_token().kind == TokenKind::CommaToken {
                //next token of comma is identifier eat comma then
                if self.peek_token(1).kind == TokenKind::IdentifierToken {
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
