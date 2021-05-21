#[derive(Debug, PartialEq, Clone)]
pub enum SyntaxKind {
    EndOfFileToken = 0,
    WhiteSpaceToken = 1,
    NewLineToken = 2,
    IdentifierToken=5,

    NumberToken = 10,
    PlusToken,
    MinusToken,
    SlashToken,
    StarToken,
    BangToken,
    EqualEqualToken,
    AmpersandAmpersandToken,
    PipePipeToken,
    BitWisePipeToken,
    BitWiseAmpersandToken,
    EqualToken,
    GreaterThanEqualToken,
    GreaterThanToken,
    SmallerThanToken,
    SmallerThanEqualToken,
    OpenParenthesisToken ,
    CloseParenthesisToken,
    CurlyOpenBracketToken,
    CurlyCloseBracketToken,
    NumberExpressionToken,
    BinaryExpressionToken,
    ParenthesizedExpressionToken,

    BadToken = 1000,
}
impl SyntaxKind {
    pub fn get_binary_precedence(&self) -> i32 {
        return match self {


            SyntaxKind::BitWiseAmpersandToken => 90,
            SyntaxKind::BitWisePipeToken => 80,
            SyntaxKind::SlashToken => 50,
            SyntaxKind::StarToken => 50,

            SyntaxKind::PlusToken => 40,
            SyntaxKind::MinusToken => 40,

            SyntaxKind::BangToken => 30,

            SyntaxKind::GreaterThanEqualToken=>15,
            SyntaxKind::GreaterThanToken=>15,
            SyntaxKind::SmallerThanEqualToken=>15,
            SyntaxKind::SmallerThanToken=>15,
            SyntaxKind::EqualEqualToken=>15,
            SyntaxKind::AmpersandAmpersandToken => 20,
            SyntaxKind::PipePipeToken => 10,

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
