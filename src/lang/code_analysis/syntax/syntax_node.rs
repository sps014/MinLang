use std::fmt::Debug;
use std::hash::Hash;
use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;

#[derive(Debug,Clone)]
pub struct ProgramNode
{
    pub functions: Vec<FunctionNode>,
}

impl ProgramNode {
    pub fn new(functions: Vec<FunctionNode>) -> ProgramNode {
        ProgramNode { functions }
    }
}

#[derive(Debug,Clone)]
pub struct FunctionNode
{
    pub name: SyntaxToken,
    pub return_type: Option<TypeLiteral>,
    pub parameters: Vec<ParameterNode>,
    pub body: Vec<StatementNode>,
}

impl FunctionNode {
    pub fn new(name: SyntaxToken, return_type: Option<TypeLiteral>, parameters: Vec<ParameterNode>, body: Vec<StatementNode>) -> FunctionNode {
        FunctionNode { name, return_type, parameters, body }
    }
}

#[derive(Debug,Clone)]
pub struct ParameterNode
{
    pub name: SyntaxToken,
    pub type_: SyntaxToken,
}
impl ParameterNode
{
    pub fn new(name: SyntaxToken, type_: SyntaxToken) -> ParameterNode {
        ParameterNode { name, type_ }
    }
}

#[derive(Debug,Clone)]
pub enum StatementNode
{
    Assignment(SyntaxToken, ExpressionNode),
    Declaration(SyntaxToken, ExpressionNode),
    FunctionInvocation(SyntaxToken, Vec<ExpressionNode>),
    Return(Option<ExpressionNode>),
    /// If condition, then body, else if co
    IfElse(ExpressionNode, Vec<StatementNode>,Vec<(ExpressionNode,Vec<StatementNode>)>, Option<Vec<StatementNode>>),
    While(ExpressionNode, Vec<StatementNode>),
    Break,
    Continue,
}

#[derive(Debug,Clone)]
pub enum ExpressionNode
{
    Number(TypeLiteral),
    Binary(Box<ExpressionNode>, SyntaxToken, Box<ExpressionNode>),
    Unary(SyntaxToken, Box<ExpressionNode>),
    Identifier(SyntaxToken),
    Parenthesized(Box<ExpressionNode>),
    FunctionCall(SyntaxToken, Vec<ExpressionNode>),
}

#[derive(Debug,Clone,PartialEq)]
pub enum TypeLiteral
{
    Integer(SyntaxToken),
    Float(SyntaxToken),
    String(SyntaxToken),
    Void,
}

impl TypeLiteral {
    pub fn get_type(&self)->String
    {
        match self {
            TypeLiteral::Integer(_) => "int",
            TypeLiteral::Float(_) => "float",
            TypeLiteral::String(_) => "string",
            TypeLiteral::Void => "void",
        }.to_string()
    }
    pub fn get_line_str(&self)->String
    {
        match self {
            TypeLiteral::Integer(token) => token.position.get_point_str(),
            TypeLiteral::Float(token) => token.position.get_point_str(),
            TypeLiteral::String(token) =>token.position.get_point_str(),
            TypeLiteral::Void => "".to_string(),
        }
    }
    pub fn from_token(token: SyntaxToken) -> Result<TypeLiteral, Error> {
        let r=match token.text.as_str()  {
            "int" => TypeLiteral::Integer(token),
            "float" => TypeLiteral::Float(token),
            "string" => TypeLiteral::String(token),
            "void" => TypeLiteral::Void,
            _ => return Err(Error::new(ErrorKind::Other,"TypeLiteral::from_token: Unexpected token kind: {:?}"))
        };
        Ok(r)
    }
}
