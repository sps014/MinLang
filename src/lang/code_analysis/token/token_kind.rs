#[derive(Debug, PartialEq, Clone,Copy,Hash,Eq)]
pub enum TokenKind
{
    EndOfFileToken,
    WhiteSpaceToken,
    BadToken,

    IdentifierToken,
    NumberToken,
    StringToken,
    
    PlusToken,
    MinusToken,
    SlashToken,
    StarToken,
    BangToken,
    ModulusToken,
    
    EqualEqualToken,
    NotEqualToken,
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

    LineCommentToken,
    BlockCommentToken,
}
