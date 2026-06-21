use std::collections::HashMap;
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
    /// Monotonic counter used to generate unique synthetic local names for `for-each` lowering.
    foreach_counter: usize,
    /// Declared type aliases (`type Foo = Bar;`). Resolved (erased) at parse time so the rest of
    /// the compiler never sees the alias name.
    type_aliases: HashMap<String, Type>,
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
            foreach_counter:0,
            type_aliases: HashMap::new(),
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
    /// True if the current token can close a generic argument list: either a plain `>` or the
    /// first half of a `>>` (`ShiftRightToken`), which appears when two generic lists end
    /// together, e.g. the `>>` in `Box<Box<int>>`.
    fn is_generic_close(&self) -> bool {
        matches!(
            self.current_token().kind,
            TokenKind::GreaterThanToken | TokenKind::ShiftRightToken
        )
    }
    /// Consumes one generic-list closing `>`. When the current token is `>>` it is split in
    /// place: one `>` is consumed conceptually and the pending token is rewritten to a single
    /// `>` so the enclosing generic list can close on the next call. Reports an error if neither
    /// is present.
    fn match_generic_close(&mut self) {
        let token = self.current_token();
        match token.kind {
            TokenKind::GreaterThanToken => {
                self.next_token();
            }
            TokenKind::ShiftRightToken => {
                // Rewrite `>>` to a single `>` and stay put so the outer close consumes it.
                if self.current_token_index < self.tokens.len() {
                    let remaining = &mut self.tokens[self.current_token_index];
                    remaining.kind = TokenKind::GreaterThanToken;
                    remaining.text = ">".to_string();
                }
            }
            _ => {
                self.match_token(TokenKind::GreaterThanToken);
            }
        }
    }
    /// Recovery guard for token-consuming loops: if no token has been consumed since `mark`,
    /// skip one token so malformed input surfaces an error (already reported by the failing
    /// `match_token`) instead of spinning forever. Never advances past end-of-file.
    fn ensure_progress(&mut self, mark: usize) {
        if self.current_token_index == mark
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            self.next_token();
        }
    }
    /// Lookahead over a balanced generic argument list whose first argument token is at peek
    /// offset `start` (i.e. the opening `<` was already seen). Tracks nesting so multi-argument
    /// and nested generics (`Pair<Box<int>, int>`, `Box<Box<int>>`) are handled, treating `>>`
    /// as two closing `>`. Returns the peek offset of the token right after the matching close,
    /// or `None` if a `;`/end-of-file is hit first (not a generic list). Used only to
    /// disambiguate generic calls/instantiations from `<`/`>` comparisons.
    fn scan_generic_args(&self, mut i: usize) -> Option<usize> {
        let mut depth: i32 = 1;
        while self.peek_token(i).kind != TokenKind::EndOfFileToken {
            match self.peek_token(i).kind {
                TokenKind::SmallerThanToken => depth += 1,
                TokenKind::GreaterThanToken => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                TokenKind::ShiftRightToken => {
                    depth -= 2;
                    if depth <= 0 {
                        return Some(i + 1);
                    }
                }
                TokenKind::SemicolonToken => return None,
                _ => {}
            }
            i += 1;
        }
        None
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
        let mut enums=vec![];
        
        while self.current_token().kind == TokenKind::ImportToken {
            imports.push(self.parse_import()?);
        }
        
        while self.current_token().kind!=TokenKind::EndOfFileToken
        {
            if self.current_token().kind == TokenKind::StructToken || (self.current_token().kind == TokenKind::PubToken && self.peek_token(1).kind == TokenKind::StructToken) {
                let struct_decl = self.parse_struct_declaration()?;
                structs.push(struct_decl);
            } else if self.current_token().kind == TokenKind::EnumToken {
                enums.push(self.parse_enum_declaration()?);
            } else if self.current_token().kind == TokenKind::TypeToken {
                self.parse_type_alias()?;
            } else if self.current_token().kind == TokenKind::FunToken || self.current_token().kind == TokenKind::AtToken || self.current_token().kind == TokenKind::ExternToken || (self.current_token().kind == TokenKind::PubToken && self.peek_token(1).kind == TokenKind::FunToken) {
                let function=self.parse_function()?;
                functions.push(function);
            } else {
                let cur = self.current_token();
                self.diagnostics.report_error(
                    format!("Expected function, struct, or enum declaration but found {:?}", cur.kind),
                    Some(cur.position.clone())
                );
                self.next_token();
            }
        }
        Ok(ProgramNode::new(imports, structs, functions, enums))
    }

    /// Parses a type alias: `type Name = ExistingType;`. The alias is recorded and resolved
    /// (erased) during `parse_type`, so it must be declared before use.
    fn parse_type_alias(&mut self) -> Result<(), Error> {
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
    fn parse_enum_declaration(&mut self) -> Result<crate::lang::code_analysis::syntax::nodes::EnumDeclarationNode, Error> {
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
        Ok(crate::lang::code_analysis::syntax::nodes::EnumDeclarationNode::new(name, members))
    }
    
    /// Parses a struct declaration
    fn parse_struct_declaration(&mut self) -> Result<crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>, Error> {
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
                fields.push(crate::lang::code_analysis::syntax::nodes::struct_node::StructFieldNode {
                    name: field_name,
                    type_token: field_type_token,
                });
            }
            self.ensure_progress(iter);
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
                let mut args = Vec::new();
                while !self.is_generic_close() && self.current_token().kind != TokenKind::EndOfFileToken {
                    let iter = self.current_token_index;
                    args.push(self.parse_type()?);
                    if self.current_token().kind == TokenKind::CommaToken {
                        self.match_token(TokenKind::CommaToken);
                    }
                    self.ensure_progress(iter);
                }
                self.match_generic_close();
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
    fn parse_js_attribute_args(&mut self) -> (Option<String>, Option<String>) {
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
    fn parse_formal_parameters(&mut self)->Result<Vec<ParameterNode>,Error>
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

    /// Parses a block of statements enclosed in curly braces
    fn parse_block(&mut self)->Result<&'a [StatementNode<'a>],Error>
    {
        //eat the open curly brace
        self.match_token(TokenKind::CurlyOpenBracketToken);
        let mut statements=vec![];
        while self.current_token().kind!=TokenKind::CurlyCloseBracketToken
            && self.current_token().kind!=TokenKind::EndOfFileToken
        {
            let iter = self.current_token_index;
            let statement=self.parse_statement()?;
            statements.push(statement);
            self.ensure_progress(iter);
        }
        //eat the close curly brace
        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(self.arena.alloc_slice_fill_iter(statements))
    }
    /// Parses a single statement based on the current token
    /// Maps a compound-assignment token (`+=`, `-=`, ...) to the plain binary operator it expands
    /// to. Returns `None` for any other token kind.
    fn compound_assign_operator(kind: TokenKind) -> Option<TokenKind> {
        match kind {
            TokenKind::PlusEqualToken => Some(TokenKind::PlusToken),
            TokenKind::MinusEqualToken => Some(TokenKind::MinusToken),
            TokenKind::StarEqualToken => Some(TokenKind::StarToken),
            TokenKind::SlashEqualToken => Some(TokenKind::SlashToken),
            TokenKind::ModulusEqualToken => Some(TokenKind::ModulusToken),
            _ => None,
        }
    }

    /// Computes the integer code point of a char literal token (text still includes the
    /// surrounding single quotes), resolving common escape sequences.
    fn char_literal_value(text: &str) -> i32 {
        let inner = text.trim_matches('\'');
        let mut chars = inner.chars();
        match chars.next() {
            Some('\\') => match chars.next() {
                Some('n') => '\n' as i32,
                Some('t') => '\t' as i32,
                Some('r') => '\r' as i32,
                Some('0') => 0,
                Some('\\') => '\\' as i32,
                Some('\'') => '\'' as i32,
                Some('"') => '"' as i32,
                Some(other) => other as i32,
                None => 0,
            },
            Some(c) => c as i32,
            None => 0,
        }
    }

    /// The source text for a plain binary operator token, used when synthesizing nodes for
    /// desugared compound assignments and increments.
    fn operator_text(kind: TokenKind) -> String {
        match kind {
            TokenKind::PlusToken => "+",
            TokenKind::MinusToken => "-",
            TokenKind::StarToken => "*",
            TokenKind::SlashToken => "/",
            TokenKind::ModulusToken => "%",
            _ => "",
        }.to_string()
    }

    /// Builds the appropriate assignment statement for a parsed lvalue expression and value.
    fn make_assignment_statement(&mut self, target: ExpressionNode<'a>, value: ExpressionNode<'a>, cur: &SyntaxToken) -> Result<StatementNode<'a>, Error> {
        match target {
            ExpressionNode::Identifier(id) => Ok(StatementNode::Assignment(id, value)),
            ExpressionNode::IndexAccess(arr, idx) => Ok(StatementNode::IndexAssignment(arr, idx, value)),
            ExpressionNode::MemberAccess(obj, member) => Ok(StatementNode::MemberAssignment(obj, member, value)),
            _ => {
                self.diagnostics.report_error(
                    format!("Invalid assignment target"),
                    Some(cur.position.clone())
                );
                Ok(StatementNode::Break(None))
            }
        }
    }

    fn parse_statement(&mut self)->Result<StatementNode<'a>,Error>
    {
        let cur = self.current_token();
        match cur.kind {
            TokenKind::LetToken | TokenKind::ConstToken => Ok(self.parse_declaration()?),
            TokenKind::ReturnToken => Ok(self.parse_return()?),
            TokenKind::IfToken => Ok(self.parse_if_else()?),
            TokenKind::WhileToken => Ok(self.parse_while()?),
            TokenKind::DoToken => Ok(self.parse_do_while()?),
            TokenKind::ForToken => Ok(self.parse_for()?),
            TokenKind::SwitchToken => Ok(self.parse_switch()?),
            TokenKind::BreakToken => Ok(self.parse_break()?),
            TokenKind::ContinueToken => Ok(self.parse_continue()?),
            // A loop label: `name: while (...) { ... }` (also `for`/`do`).
            TokenKind::IdentifierToken if self.peek_token(1).kind == TokenKind::ColonToken => {
                let label = self.match_token(TokenKind::IdentifierToken);
                self.match_token(TokenKind::ColonToken);
                let inner = self.parse_statement()?;
                let inner_ref = self.arena.alloc(inner);
                Ok(StatementNode::Labeled(label.text, inner_ref))
            },
            TokenKind::IdentifierToken => {
                // Parse an expression first
                let expr = self.parse_primary_expression()?;
                
                if self.current_token().kind == TokenKind::EqualToken {
                    self.match_token(TokenKind::EqualToken);
                    let value = self.parse_expression(0)?;
                    self.match_token(TokenKind::SemicolonToken);
                    self.make_assignment_statement(expr, value, &cur)
                } else if let Some(plain_kind) = Self::compound_assign_operator(self.current_token().kind) {
                    // Compound assignment `target OP= rhs` desugars to `target = target OP (rhs)`.
                    let op_tok = self.next_token();
                    let rhs = self.parse_expression(0)?;
                    self.match_token(TokenKind::SemicolonToken);
                    let plain_token = SyntaxToken::new(plain_kind, op_tok.position.clone(), Self::operator_text(plain_kind));
                    let left_operand = self.arena.alloc(expr.clone());
                    let value = ExpressionNode::Binary(left_operand, plain_token, self.arena.alloc(rhs));
                    self.make_assignment_statement(expr, value, &cur)
                } else if matches!(self.current_token().kind, TokenKind::PlusPlusToken | TokenKind::MinusMinusToken) {
                    // `target++` / `target--` desugars to `target = target +/- 1`.
                    let op_tok = self.next_token();
                    let plain_kind = if op_tok.kind == TokenKind::PlusPlusToken { TokenKind::PlusToken } else { TokenKind::MinusToken };
                    self.match_token(TokenKind::SemicolonToken);
                    let plain_token = SyntaxToken::new(plain_kind, op_tok.position.clone(), Self::operator_text(plain_kind));
                    let one_token = SyntaxToken::new(TokenKind::NumberToken, op_tok.position.clone(), "1".to_string());
                    let one = ExpressionNode::Literal(Type::Integer(one_token));
                    let left_operand = self.arena.alloc(expr.clone());
                    let value = ExpressionNode::Binary(left_operand, plain_token, self.arena.alloc(one));
                    self.make_assignment_statement(expr, value, &cur)
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
                            Ok(StatementNode::Break(None)) 
                        }
                    }
                } else {
                    self.diagnostics.report_error(
                        format!("Unexpected token {:?} after expression", self.current_token().kind),
                        Some(self.current_token().position.clone())
                    );
                    self.next_token(); // skip the token
                    Ok(StatementNode::Break(None)) // dummy
                }
            },
            _ => {
                self.diagnostics.report_error(
                    format!("Expected statement but found {:?} at {}", cur.text, cur.position.get_point_str()),
                    Some(cur.position.clone())
                );
                self.next_token(); // skip the token
                Ok(StatementNode::Break(None)) // dummy
            }
        }
    }

    /// Parses a variable declaration (e.g., `let x = 5;` or `let x: int[] = [1];`)
    fn parse_declaration(&mut self)->Result<StatementNode<'a>,Error>
    {
        // Consume `let` or `const`; `const` marks the binding immutable.
        let is_const = self.current_token().kind == TokenKind::ConstToken;
        if is_const {
            self.match_token(TokenKind::ConstToken);
        } else {
            self.match_token(TokenKind::LetToken);
        }
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
        Ok(StatementNode::Declaration(identifier, type_annotation, expression, is_const))
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
                    let mut args = Vec::new();
                    while !self.is_generic_close() && self.current_token().kind != TokenKind::EndOfFileToken {
                        let iter = self.current_token_index;
                        args.push(self.parse_type()?);
                        if self.current_token().kind == TokenKind::CommaToken {
                            self.match_token(TokenKind::CommaToken);
                        }
                        self.ensure_progress(iter);
                    }
                    self.match_generic_close();
                    generic_arguments = Some(args);
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
                                let mut args = Vec::new();
                                while !self.is_generic_close() && self.current_token().kind != TokenKind::EndOfFileToken {
                                    let iter = self.current_token_index;
                                    args.push(self.parse_type()?);
                                    if self.current_token().kind == TokenKind::CommaToken {
                                        self.match_token(TokenKind::CommaToken);
                                    }
                                    self.ensure_progress(iter);
                                }
                                self.match_generic_close();
                                generic_args = Some(args);
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
    fn parse_invocation_expression(&mut self)->Result<ExpressionNode<'a>,Error>
    {
        let function_name=self.match_token(TokenKind::IdentifierToken);
        
        let mut generic_arguments = None;
        if self.current_token().kind == TokenKind::SmallerThanToken {
            self.match_token(TokenKind::SmallerThanToken);
            let mut args = Vec::new();
            while !self.is_generic_close() && self.current_token().kind != TokenKind::EndOfFileToken {
                let iter = self.current_token_index;
                args.push(self.parse_type()?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
                self.ensure_progress(iter);
            }
            self.match_generic_close();
            generic_arguments = Some(args);
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

        // For-each form: `for (let <var> in <iterable>) { ... }`.
        if self.current_token().kind == TokenKind::LetToken
            && self.peek_token(1).kind == TokenKind::IdentifierToken
            && self.peek_token(2).kind == TokenKind::InToken
        {
            self.match_token(TokenKind::LetToken);
            let element = self.match_token(TokenKind::IdentifierToken);
            self.match_token(TokenKind::InToken);
            let iterable = self.parse_expression(0)?;
            self.match_token(TokenKind::CloseParenthesisToken);
            let body = self.parse_block()?;

            let n = self.foreach_counter;
            self.foreach_counter += 1;
            let index_name = format!("__foreach_idx_{}", n);
            let array_name = format!("__foreach_arr_{}", n);
            return Ok(StatementNode::ForEach(element, iterable, index_name, array_name, body));
        }

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
                    StatementNode::Break(None)
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
    /// Parses a do-while loop: `do { body } while (condition);`.
    fn parse_do_while(&mut self)->Result<StatementNode<'a>,Error>
    {
        self.match_token(TokenKind::DoToken);
        let body=self.parse_block()?;
        self.match_token(TokenKind::WhileToken);
        self.match_token(TokenKind::OpenParenthesisToken);
        let condition=self.parse_expression(0)?;
        self.match_token(TokenKind::CloseParenthesisToken);
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::DoWhile(body, condition))
    }
    /// Parses a switch statement:
    /// `switch (expr) { case v1, v2: stmt* case v3: stmt* default: stmt* }`.
    /// Each case body runs until the next `case`/`default`/`}` and there is no implicit fallthrough.
    fn parse_switch(&mut self)->Result<StatementNode<'a>,Error>
    {
        self.match_token(TokenKind::SwitchToken);
        self.match_token(TokenKind::OpenParenthesisToken);
        let subject = self.parse_expression(0)?;
        self.match_token(TokenKind::CloseParenthesisToken);
        self.match_token(TokenKind::CurlyOpenBracketToken);

        let mut cases: Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])> = Vec::new();
        let mut default_body: Option<&'a [StatementNode<'a>]> = None;

        while self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let iter = self.current_token_index;
            if self.current_token().kind == TokenKind::CaseToken {
                self.match_token(TokenKind::CaseToken);
                // One or more comma-separated label expressions.
                let mut labels = vec![self.parse_expression(0)?];
                while self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                    labels.push(self.parse_expression(0)?);
                }
                self.match_token(TokenKind::ColonToken);
                let body = self.parse_case_body()?;
                cases.push((labels, body));
            } else if self.current_token().kind == TokenKind::DefaultToken {
                self.match_token(TokenKind::DefaultToken);
                self.match_token(TokenKind::ColonToken);
                let body = self.parse_case_body()?;
                if default_body.is_some() {
                    self.diagnostics.report_error(
                        "Multiple 'default' clauses in switch statement".to_string(),
                        Some(self.current_token().position.clone()),
                    );
                }
                default_body = Some(body);
            } else {
                self.diagnostics.report_error(
                    format!("Expected 'case' or 'default' in switch body but found {:?}", self.current_token().text),
                    Some(self.current_token().position.clone()),
                );
                self.next_token();
            }
            self.ensure_progress(iter);
        }

        self.match_token(TokenKind::CurlyCloseBracketToken);
        Ok(StatementNode::Switch(subject, cases, default_body))
    }

    /// Parses the statements of a single `case`/`default` clause, up to (but not consuming) the
    /// next `case`, `default`, or the closing `}`.
    fn parse_case_body(&mut self)->Result<&'a [StatementNode<'a>],Error>
    {
        let mut statements = vec![];
        while self.current_token().kind != TokenKind::CaseToken
            && self.current_token().kind != TokenKind::DefaultToken
            && self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let iter = self.current_token_index;
            statements.push(self.parse_statement()?);
            self.ensure_progress(iter);
        }
        Ok(self.arena.alloc_slice_fill_iter(statements))
    }

    /// Parses a break statement, with an optional target label: `break;` or `break outer;`.
    fn parse_break(&mut self)->Result<StatementNode<'a>,Error>
    {
        self.match_token(TokenKind::BreakToken);
        let label = if self.current_token().kind == TokenKind::IdentifierToken {
            Some(self.match_token(TokenKind::IdentifierToken).text)
        } else {
            None
        };
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Break(label))
    }
    /// Parses a continue statement, with an optional target label: `continue;` or `continue outer;`.
    fn parse_continue(&mut self)->Result<StatementNode<'a>,Error>
    {
        self.match_token(TokenKind::ContinueToken);
        let label = if self.current_token().kind == TokenKind::IdentifierToken {
            Some(self.match_token(TokenKind::IdentifierToken).text)
        } else {
            None
        };
        self.match_token(TokenKind::SemicolonToken);
        Ok(StatementNode::Continue(label))
    }
}

#[cfg(test)]
#[path = "tests/parser_tests.rs"]
mod tests;
