use super::*;
use pretty_assertions::assert_eq;

#[test]
fn test_lex_keywords() {
    let mut lexer = Lexer::new("let fun if else for while return break continue import pub".to_string());
    let mut diagnostics = DiagnosticBag::new(None);
    let tokens = lexer.lex_all(&mut diagnostics);
    
    assert_eq!(diagnostics.has_errors(), false);
    let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![
        TokenKind::LetToken,
        TokenKind::FunToken,
        TokenKind::IfToken,
        TokenKind::ElseToken,
        TokenKind::ForToken,
        TokenKind::WhileToken,
        TokenKind::ReturnToken,
        TokenKind::BreakToken,
        TokenKind::ContinueToken,
        TokenKind::ImportToken,
        TokenKind::PubToken,
        TokenKind::EndOfFileToken,
    ]);
}

#[test]
fn test_lex_operators() {
    let mut lexer = Lexer::new("+ - * / % == != > < >= <= && || ! = [ ] { } ( ) , . : ;".to_string());
    let mut diagnostics = DiagnosticBag::new(None);
    let tokens = lexer.lex_all(&mut diagnostics);
    
    assert_eq!(diagnostics.has_errors(), false);
    let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![
        TokenKind::PlusToken,
        TokenKind::MinusToken,
        TokenKind::StarToken,
        TokenKind::SlashToken,
        TokenKind::ModulusToken,
        TokenKind::EqualEqualToken,
        TokenKind::NotEqualToken,
        TokenKind::GreaterThanToken,
        TokenKind::SmallerThanToken,
        TokenKind::GreaterThanEqualToken,
        TokenKind::SmallerThanEqualToken,
        TokenKind::AmpersandAmpersandToken,
        TokenKind::PipePipeToken,
        TokenKind::BangToken,
        TokenKind::EqualToken,
        TokenKind::OpenBracketToken,
        TokenKind::CloseBracketToken,
        TokenKind::CurlyOpenBracketToken,
        TokenKind::CurlyCloseBracketToken,
        TokenKind::OpenParenthesisToken,
        TokenKind::CloseParenthesisToken,
        TokenKind::CommaToken,
        TokenKind::DotToken,
        TokenKind::ColonToken,
        TokenKind::SemicolonToken,
        TokenKind::EndOfFileToken,
    ]);
}

#[test]
fn test_lex_literals() {
    let mut lexer = Lexer::new("42 3.14 \"hello\" true false".to_string());
    let mut diagnostics = DiagnosticBag::new(None);
    let tokens = lexer.lex_all(&mut diagnostics);
    
    assert_eq!(diagnostics.has_errors(), false);
    let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![
        TokenKind::NumberToken,
        TokenKind::NumberToken,
        TokenKind::StringToken,
        TokenKind::BooleanToken,
        TokenKind::BooleanToken,
        TokenKind::EndOfFileToken,
    ]);
}

#[test]
fn test_lex_bad_token() {
    let mut lexer = Lexer::new("let # = 5;".to_string());
    let mut diagnostics = DiagnosticBag::new(None);
    let tokens = lexer.lex_all(&mut diagnostics);
    
    assert_eq!(diagnostics.has_errors(), true);
    assert_eq!(diagnostics.diagnostics.len(), 1);
    assert_eq!(diagnostics.diagnostics[0].message, "unexpected token '#'");
    
    // The bad token should be skipped
    let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![
        TokenKind::LetToken,
        TokenKind::EqualToken,
        TokenKind::NumberToken,
        TokenKind::SemicolonToken,
        TokenKind::EndOfFileToken,
    ]);
}
