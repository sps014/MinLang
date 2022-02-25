mod lang;
use std::{collections::HashMap, io::stdin};
use std::error::Error;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::semantic_analysis::analyzer::Anaylzer;

fn main() {

    let input_text=r"fun abc(test:int,alpha:float)
    {
        let b=5.0;
        let a=4.7+1.5+b;


    }";

    let lexer= Lexer::new(input_text.to_string());
    let mut parser=Parser::new(lexer);
    let mut analyzer=Anaylzer::new(parser);
    let result=analyzer.analyze();
    match result{
        Ok(()) =>
            println!("No errors found"),

        Err(e) => println!("error: {:?}",e.description()),
    }
}

