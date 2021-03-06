use crate::lang::code_analysis::token::token_kind::TokenKind;

impl TokenKind {
    pub fn get_binary_precedence(&self) -> i32 {
        return match self {
            TokenKind::BitWiseAmpersandToken => 90,
            TokenKind::BitWisePipeToken => 80,

            TokenKind::ModulusToken => 55,

            TokenKind::SlashToken => 50,
            TokenKind::StarToken => 50,

            TokenKind::PlusToken => 40,
            TokenKind::MinusToken => 40,

            TokenKind::BangToken => 30,

            TokenKind::GreaterThanEqualToken => 15,
            TokenKind::GreaterThanToken => 15,
            TokenKind::SmallerThanEqualToken => 15,
            TokenKind::SmallerThanToken => 15,
            TokenKind::EqualEqualToken => 15,
            TokenKind::AmpersandAmpersandToken => 20,
            TokenKind::PipePipeToken => 10,

            _ => 0,
        };
    }
    pub fn get_unary_precedence(&self) -> i32 {
        return match self {
            TokenKind::PlusToken => 6,
            TokenKind::MinusToken => 6,
            TokenKind::BangToken => 6,
            _ => 0,
        };
    }
}