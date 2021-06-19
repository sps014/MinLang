mod lang;
use crate::lang::code_analysis::evaluator::Evaluator;
use lang::code_analysis::parser::*;
use std::{collections::HashMap, io::stdin};

fn main() {
    let mut variables: HashMap<String, i32> = HashMap::new();

    loop {
        println!("\n");

        let mut line: String = String::new();
        stdin().read_line(&mut line).unwrap();
        let e = line.trim_end().to_string();
        call(e, &mut variables);
    }
}
fn call(input: String, variables: &mut HashMap<String, i32>) {
    let mut p = Parser::new(input.as_str());
    let tree = p.parse();
    //println!("{:?}",tree.root);
    let mut e = Evaluator::new(*tree.root);
    match e.evaluate(variables) {
        Ok(n) => {}
        Err(e) => println!("Error occurred {}", e.to_string()),
    }
}
