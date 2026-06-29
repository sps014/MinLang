//! Classifies lexer tokens into semantic-highlighting categories. The lexer skips whitespace
//! and comments, so those are handled by the editor's Monarch grammar; this provides the
//! richer keyword/type/identifier/literal classification used for semantic tokens.

use dream::driver::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::token::token_kind::TokenKind;

use crate::position::{LineIndex, Range};

#[derive(Debug, Clone)]
pub struct TokenOut {
    pub range: Range,
    pub kind: &'static str,
}

/// The ordered set of semantic token categories this analyzer emits. The web layer turns this
/// into a Monaco semantic-tokens legend (index = position in this slice).
pub const TOKEN_LEGEND: [&str; 6] = ["keyword", "type", "string", "number", "operator", "variable"];

pub fn classify(text: &str) -> Vec<TokenOut> {
    let line_index = LineIndex::new(text);
    let mut scratch = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let tokens = lexer.lex_all(&mut scratch);

    tokens
        .into_iter()
        .filter_map(|token| {
            // `this` lexes as an identifier but reads as a keyword inside methods.
            let kind = if token.kind == TokenKind::IdentifierToken && token.text == "this" {
                "keyword"
            } else {
                category(token.kind)?
            };
            let span = token.position;
            Some(TokenOut { range: line_index.range(span.start, span.end), kind })
        })
        .collect()
}

/// Maps a lexical token kind to a highlighting category, or `None` for tokens that carry no
/// useful color (end-of-file, punctuation that the grammar already styles, bad tokens).
fn category(kind: TokenKind) -> Option<&'static str> {
    use TokenKind::*;
    let category = match kind {
        IdentifierToken => "variable",
        DataTypeToken => "type",
        NumberToken => "number",
        StringToken | CharToken => "string",
        BooleanToken | NullToken => "keyword",
        IfToken | ElseToken | ForToken | WhileToken | DoToken | ReturnToken | BreakToken
        | ContinueToken | LetToken | ConstToken | FunToken | StaticToken | ImportToken
        | ExportToken | ExternToken | ClassToken | ExtendToken | IsToken | InToken | EnumToken
        | TypeToken | SwitchToken | CaseToken | DefaultToken => "keyword",
        PlusToken | MinusToken | SlashToken | StarToken | BangToken | ModulusToken
        | PlusEqualToken | MinusEqualToken | StarEqualToken | SlashEqualToken
        | ModulusEqualToken | PlusPlusToken | MinusMinusToken | EqualEqualToken
        | NotEqualToken | AmpersandAmpersandToken | PipePipeToken | BitWisePipeToken
        | BitWiseAmpersandToken | BitWiseXorToken | ShiftLeftToken | ShiftRightToken
        | QuestionQuestionToken | EqualToken | GreaterThanEqualToken | GreaterThanToken
        | SmallerThanToken | SmallerThanEqualToken => "operator",
        _ => return None,
    };
    Some(category)
}
