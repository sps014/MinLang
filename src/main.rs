mod lang;
use lang::code_analysis::parser::*;

fn main() {
    call();
}
fn call() {
    let input = "2 +3+ 5  -  7*978+56";
    let mut p = Parser::new(input);
    let tree=p.parse();
    let mk=tree;
}
