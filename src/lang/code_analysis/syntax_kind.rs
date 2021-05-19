#[derive(Debug, PartialEq, Clone)]
pub enum SyntaxKind {
    EndOfFileToken = 0,
    WhiteSpaceToken = 1,
    NewLineToken = 2,

    NumberToken = 10,
    PlusToken = 50,
    MinusToken = 51,
    SlashToken = 52,
    StarToken = 53,
    BangToken = 54,
    EqualEqualToken = 55,
    AmpersandAmpersandToken = 56,
    PipePipeToken = 57,
    BitWisePipeToken = 58,
    BitWiseAmpersandToken = 59,

    OpenParenthesisToken = 70,
    CloseParenthesisToken = 71,

    NumberExpressionToken = 100,
    BinaryExpressionToken = 101,
    ParenthesizedExpressionToken = 102,

    BadToken = 1000,
}
impl SyntaxKind {
    pub fn get_binary_precedence(&self) -> i32 {
        return match self {
            SyntaxKind::BitWiseAmpersandToken => 9,
            SyntaxKind::BitWisePipeToken => 8,
            SyntaxKind::SlashToken => 5,
            SyntaxKind::StarToken => 5,

            SyntaxKind::PlusToken => 4,
            SyntaxKind::MinusToken => 4,

            SyntaxKind::EqualEqualToken => 3,
            SyntaxKind::BangToken => 3,

            SyntaxKind::AmpersandAmpersandToken => 2,
            SyntaxKind::PipePipeToken => 1,

            _ => 0,
        };
    }
    pub fn get_unary_precedence(&self) -> i32 {
        return match self {
            SyntaxKind::PlusToken => 6,
            SyntaxKind::MinusToken => 6,
            SyntaxKind::BangToken => 6,
            _ => 0,
        };
    }
}
