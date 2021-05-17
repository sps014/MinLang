mod lang;
use lang::code_analysis::lexer::*;

use crate::lang::code_analysis::syntax_kind::SyntaxKind;
fn main() {
    call();
}
fn call() {
    let mut l = Lexer::new("2+3+5-7 * 978 - 56");
    while true {
        let t = l.next_token();
        l.next();
        println!("{:?}", t);
        if t.kind == SyntaxKind::EndOfFileToken {
            break;
        }
    }
}
