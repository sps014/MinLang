mod lang;
use std::{collections::HashMap, io::stdin};
use std::error::Error;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;

fn main() {

    let lexer= Lexer::new(r"fun abc(test:int,alpha:float)
    {
        let a= b+c*d;
    }".to_string());
    let mut parser=Parser::new(lexer);
    let ast=parser.parse();
    match ast {
        Ok(ast) =>
            println!("{:?}",ast),

        Err(e) => println!("error: {:?}",e.description()),
    }
}

