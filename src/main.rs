mod lang;
use std::{collections::HashMap, io::stdin};
use crate::lang::code_analysis::lexer::Lexer;

fn main() {

    let mut lexer= Lexer::new("[ ] + 56 . ,".to_string());
    println!("{:?}", lexer.lex_all());
}

