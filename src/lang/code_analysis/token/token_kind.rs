#[derive(Debug, PartialEq, Clone,Copy,Hash,Eq)]
pub enum TokenKind
{
    EndOfFileToken,
    WhiteSpaceToken,
    BadToken,

    IdentifierToken,
    NumberToken,

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

    SemicolonToken,
    ColonToken,
    CommaToken,
    DotToken,
    
    OpenParenthesisToken,
    CloseParenthesisToken,
    CurlyOpenBracketToken,
    CurlyCloseBracketToken,

    IfToken,
    ElseToken,
    ForToken,
    WhileToken,
    ReturnToken,
    BreakToken,
    ContinueToken,
    LetToken,
    FunToken,
    DataTypeToken,
}
