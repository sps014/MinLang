#[derive(Debug, PartialEq)]
pub enum SyntaxKind {
    EndOfFileToken = 0,
    WhiteSpaceToken = 1,
    NewLineToken = 2,

    NumberToken = 10,
    OperatorToken = 50,
    BadToken = 1000,
}
