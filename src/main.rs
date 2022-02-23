mod lang;
use std::{collections::HashMap, io::stdin};
use crate::lang::code_analysis::lexer::Lexer;

fn main() {

    let mut lexer= Lexer::new(r"{
}".to_string());
    let tokens=lexer.lex_all();
    for token in tokens {
        println!("{:?}",token);
    }
}

