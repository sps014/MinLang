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

mod declarations;
mod statements;
mod expressions;

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
    /// Parses a comma-separated list of generic type arguments, assuming the opening `<`
    /// has already been consumed, and consumes the matching closing `>`/`>>`. Used at every
    /// site that accepts generic arguments (type annotations, function/method calls, struct
    /// instantiation) so the loop and recovery logic live in one place.
    fn parse_generic_args(&mut self) -> Result<Vec<Type>, Error> {
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
        Ok(args)
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

}

#[cfg(test)]
#[path = "../tests/parser_tests.rs"]
mod tests;
