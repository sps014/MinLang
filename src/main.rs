mod lang;
use lang::code_analysis::lexer::*;
use lang::code_analysis::parser::*;
use crate::lang::code_analysis::syntax_kind::SyntaxKind;

fn main() {
    call();
}
fn call() {
    let s=SyntaxKind::PlusToken;
    let m=s.clone();
    println!("{:?}",m);
    let  input = "2 +3+ 5  -  7
    *978-56";
    let p=Parser::new(input);
}
