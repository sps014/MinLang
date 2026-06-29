use bumpalo::Bump;
use dream::syntax::lexer::Lexer;
use dream::syntax::parser::Parser;
use dream::driver::diagnostics::DiagnosticBag;

fn main() {
    let text = "let x = 5;\nfun foo() { }";
    let arena = Bump::new();
    let mut scratch = DiagnosticBag::new(None);
    let lexer = Lexer::new(text.to_string());
    let mut parser = Parser::new(lexer, &arena, &mut scratch);
    let res = parser.parse();
    println!("{:?}", res.is_ok());
    println!("{:?}", scratch.diagnostics);
}
