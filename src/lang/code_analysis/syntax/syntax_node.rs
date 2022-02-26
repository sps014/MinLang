use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

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
    pub return_type: Option<Type>,
    pub parameters: Vec<ParameterNode>,
    pub body: Vec<StatementNode>,
}

impl FunctionNode {
    pub fn new(name: SyntaxToken, return_type: Option<Type>, parameters: Vec<ParameterNode>, body: Vec<StatementNode>) -> FunctionNode {
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
    Literal(Type),
    Binary(Box<ExpressionNode>, SyntaxToken, Box<ExpressionNode>),
    Unary(SyntaxToken, Box<ExpressionNode>),
    Identifier(SyntaxToken),
    Parenthesized(Box<ExpressionNode>),
    FunctionCall(SyntaxToken, Vec<ExpressionNode>),
}

#[derive(Debug,Clone,PartialEq)]
pub enum Type
{
    Integer(SyntaxToken),
    Float(SyntaxToken),
    String(SyntaxToken),
    Void,
}

impl Type {
    pub fn get_type(&self)->String
    {
        match self {
            Type::Integer(_) => "int",
            Type::Float(_) => "float",
            Type::String(_) => "string",
            Type::Void => "void",
        }.to_string()
    }
    pub fn get_line_str(&self)->String
    {
        match self {
            Type::Integer(token) => token.position.get_point_str(),
            Type::Float(token) => token.position.get_point_str(),
            Type::String(token) =>token.position.get_point_str(),
            Type::Void => "".to_string(),
        }
    }
    pub fn from_token(token: SyntaxToken) -> Result<Type, Error> {
        let r=match token.text.as_str()  {
            "int" => Type::Integer(token),
            "float" => Type::Float(token),
            "string" => Type::String(token),
            "void" => Type::Void,
            _ => return Err(Error::new(ErrorKind::Other,"TypeLiteral::from_token: Unexpected token kind: {:?}"))
        };
        Ok(r)
    }
}
