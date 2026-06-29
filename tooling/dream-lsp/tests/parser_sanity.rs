use bumpalo::Bump;
use dream::driver::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::parser::Parser;

#[test]
fn test_parser_sanity() {
    let text = "fun main() { let x = 5; }";
    let arena = Bump::new();
    let mut scratch = DiagnosticBag::new(None);
    let lexer = Lexer::new(text.to_string());
    let mut parser = Parser::new(lexer, &arena, &mut scratch);
    let res = parser.parse();
    assert!(res.is_ok(), "Parser failed!");
    assert!(scratch.diagnostics.is_empty(), "Parser had diagnostics!");
}

#[test]
fn test_parser_error() {
    let text = "fun main() { let x = ; }"; // Missing expression
    let arena = Bump::new();
    let mut scratch = DiagnosticBag::new(None);
    let lexer = Lexer::new(text.to_string());
    let mut parser = Parser::new(lexer, &arena, &mut scratch);
    let res = parser.parse();

    // We expect it to have diagnostics. But does it return Ok or Err?
    println!("Diagnostics: {:?}", scratch.diagnostics);
    assert!(res.is_ok(), "Parser returned Err instead of recovering!");
}
