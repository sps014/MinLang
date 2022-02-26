mod lang;
use std::{collections::HashMap, io::stdin};
use std::error::Error;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::semantic_analysis::analyzer::Anaylzer;
fn main() {

    let input_text=r"
    fun get_pi(a:int) :float
    {
        return 3.14;
    }
    fun abc(test:int,alpha:float):float
    {
        let b=get_pi(5.6);
        let a=4.7+1.5+b;
        b=8.8;
        let c=6;
        if a+b
        {
          let c=c+1;
          return a;
          while c>10
          {
            break;
          }
        }


    }";

    let lexer= Lexer::new(input_text.to_string());
    let mut parser=Parser::new(lexer);
    let mut analyzer=Anaylzer::new(parser);
    let result=analyzer.analyze();
    match result{
        Ok(()) =>
            println!("No errors found"),

        Err(e) => println!("error: {}",e),
    }
}

