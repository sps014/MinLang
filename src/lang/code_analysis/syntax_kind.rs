#[derive(Debug, PartialEq,Clone)]
pub enum SyntaxKind {

    EndOfFileToken = 0,
    WhiteSpaceToken = 1,
    NewLineToken = 2,

    NumberToken = 10,
    PlusToken = 50,
    MinusToken = 51,
    SlashToken = 52,
    StarToken = 53,



    OpenParenthesisToken = 70,
    CloseParenthesisToken = 71,

    NumberExpressionToken=100,
    BinaryExpressionToken=101,
    ParenthesizedExpressionToken=102,
    
    BadToken = 1000,
}