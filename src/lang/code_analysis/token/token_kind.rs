use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub enum TokenKind
{
    EndOfFileToken,
    
    #[regex(r"[ \t\n\f]+")]
    WhiteSpaceToken,
    
    BadToken,

    #[regex("[a-zA-Z_][a-zA-Z0-9_]*")]
    IdentifierToken,
    
    #[regex(r"[0-9]+(\.[0-9]+)?([dDfF])?")]
    NumberToken,
    
    #[regex(r#""([^"\\]*(\\.[^"\\]*)*)""#)]
    StringToken,
    
    #[token("true")]
    #[token("false")]
    BooleanToken,
    
    #[token("+")]
    PlusToken,
    #[token("-")]
    MinusToken,
    #[token("/")]
    SlashToken,
    #[token("*")]
    StarToken,
    #[token("!")]
    BangToken,
    #[token("%")]
    ModulusToken,
    
    #[token("==")]
    EqualEqualToken,
    #[token("!=")]
    NotEqualToken,
    #[token("&&")]
    AmpersandAmpersandToken,
    #[token("||")]
    PipePipeToken,
    #[token("|")]
    BitWisePipeToken,
    #[token("&")]
    BitWiseAmpersandToken,
    #[token("=")]
    EqualToken,
    #[token(">=")]
    GreaterThanEqualToken,
    #[token(">")]
    GreaterThanToken,
    #[token("<")]
    SmallerThanToken,
    #[token("<=")]
    SmallerThanEqualToken,

    #[token(";")]
    SemicolonToken,
    #[token(":")]
    ColonToken,
    #[token(",")]
    CommaToken,
    #[token(".")]
    DotToken,
    #[token("?")]
    QuestionMarkToken,
    
    #[token("(")]
    OpenParenthesisToken,
    #[token(")")]
    CloseParenthesisToken,
    #[token("{")]
    CurlyOpenBracketToken,
    #[token("}")]
    CurlyCloseBracketToken,
    #[token("[")]
    OpenBracketToken,
    #[token("]")]
    CloseBracketToken,

    #[token("if")]
    IfToken,
    #[token("else")]
    ElseToken,
    #[token("for")]
    ForToken,
    #[token("while")]
    WhileToken,
    #[token("return")]
    ReturnToken,
    #[token("break")]
    BreakToken,
    #[token("continue")]
    ContinueToken,
    #[token("let")]
    LetToken,
    #[token("fun")]
    FunToken,
    #[token("import")]
    ImportToken,
    #[token("export")]
    ExportToken,
    #[token("struct")]
    StructToken,
    #[token("null")]
    NullToken,
    #[token("is")]
    IsToken,

    #[token("@")]
    AtToken,

    #[token("int")]
    #[token("float")]
    #[token("double")]
    #[token("string")]
    #[token("bool")]
    #[token("void")]
    #[token("object")]
    DataTypeToken,

    #[regex(r"//[^\n]*", allow_greedy = true)]
    LineCommentToken,
    #[regex(r"/\*[^*]*\*+(?:[^/*][^*]*\*+)*/")]
    BlockCommentToken,
}
