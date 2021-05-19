mod lang;
use lang::code_analysis::parser::*;
use crate::lang::code_analysis::evaluator::Evaluator;
use std::io::stdin;

fn main() {
    loop {
        println!("\n");
        let mut line:String=String::new();
        stdin().read_line(&mut line).unwrap();
        let e=line.trim_end().to_string();
        call(e);
    }
}
fn call(input:String) {
    let mut p = Parser::new(input.as_str());
    let tree=p.parse();
    let e=Evaluator::new(*tree.root);
    match e.evaluate()
    {
        Ok(n)=>println!("Result is {}",n),
        Err(e)=>println!("Error occurred {}",e.to_string())
    }
}
