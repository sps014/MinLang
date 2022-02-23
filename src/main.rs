mod lang;
use std::{collections::HashMap, io::stdin};
use lang::code_analysis::syntax::lexer::Lexer;

fn main() {

    let mut lexer= Lexer::new(r"{
+-
}".to_string());
    let tokens=lexer.lex_all();
    for token in tokens {
        println!("{:?}",token);
    }
}

