use min_lang::lang::code_analysis::token::token_kind::TokenKind;
use logos::Logos;

fn main() {
    let mut lexer = TokenKind::lexer("Node?");
    while let Some(token) = lexer.next() {
        println!("{:?} '{}'", token, lexer.slice());
    }
}
