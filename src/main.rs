mod lang;
use lang::code_analysis::parser::*;
use crate::lang::code_analysis::evaluator::Evaluator;
use std::error::Error;

fn main() {
    call();
}
fn call() {
    let input = "1+2*6/+3";
    let mut p = Parser::new(input);
    let tree=p.parse();
    let e=Evaluator::new(*tree.root);
    match e.evaluate()
    {
        Ok(n)=>println!("Result is {}",n),
        Err(e)=>println!("Error occurred {}",e.to_string())
    }
}
