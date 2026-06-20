use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug,Clone)]
pub struct ImportNode
{
    pub module_name: SyntaxToken,
}
impl ImportNode {
    pub fn new(module_name: SyntaxToken) -> ImportNode {
        ImportNode { module_name }
    }
}

#[derive(Debug,Clone)]
pub struct ProgramNode<'a>
{
    pub imports: Vec<ImportNode>,
    pub functions: Vec<FunctionNode<'a>>,
}

impl<'a> ProgramNode<'a> {
    pub fn new(imports: Vec<ImportNode>, functions: Vec<FunctionNode<'a>>) -> ProgramNode<'a> {
        ProgramNode { imports, functions }
    }
}

#[derive(Debug,Clone)]
pub struct FunctionNode<'a>
{
    pub name: SyntaxToken,
    pub return_type: Option<Type>,
    pub parameters: Vec<ParameterNode>,
    pub body: &'a [StatementNode<'a>],
    pub is_exported: bool,
}

impl<'a> FunctionNode<'a> {
    pub fn new(name: SyntaxToken, return_type: Option<Type>, parameters: Vec<ParameterNode>, body: &'a [StatementNode<'a>], is_exported: bool) -> FunctionNode<'a> {
        FunctionNode { name, return_type, parameters, body, is_exported }
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
pub enum StatementNode<'a>
{
    Assignment(SyntaxToken, ExpressionNode<'a>),
    Declaration(SyntaxToken, ExpressionNode<'a>),
    FunctionInvocation(SyntaxToken, Vec<ExpressionNode<'a>>),
    Return(Option<ExpressionNode<'a>>),
    /// If condition, then body, else if co
    IfElse(ExpressionNode<'a>, &'a [StatementNode<'a>], Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>, Option<&'a [StatementNode<'a>]>),
    While(ExpressionNode<'a>, &'a [StatementNode<'a>]),
    For(Option<&'a StatementNode<'a>>, Option<ExpressionNode<'a>>, Option<&'a StatementNode<'a>>, &'a [StatementNode<'a>]),
    Break,
    Continue,
}

#[derive(Debug,Clone)]
pub enum ExpressionNode<'a>
{
    Literal(Type),
    Binary(&'a ExpressionNode<'a>, SyntaxToken, &'a ExpressionNode<'a>),
    Unary(SyntaxToken, &'a ExpressionNode<'a>),
    Identifier(SyntaxToken),
    Parenthesized(&'a ExpressionNode<'a>),
    FunctionCall(SyntaxToken, Vec<ExpressionNode<'a>>),
}

#[derive(Debug,Clone,PartialEq)]
pub enum Type
{
    Integer(SyntaxToken),
    Float(SyntaxToken),
    String(SyntaxToken),
    Boolean(SyntaxToken),
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
            Type::Boolean(_) => "bool",
        }.to_string()
    }
    pub fn get_line_str(&self)->String
    {
        match self {
            Type::Integer(token) => token.position.get_point_str(),
            Type::Float(token) => token.position.get_point_str(),
            Type::String(token) =>token.position.get_point_str(),
            Type::Void => "".to_string(),
            Type::Boolean(token) => token.position.get_point_str(),
        }
    }

    pub fn from_token(token: SyntaxToken) -> Result<Type, Error> {
        let r=match token.text.as_str()  {
            "int" => Type::Integer(token),
            "float" => Type::Float(token),
            "string" => Type::String(token),
            "void" => Type::Void,
            "bool" => Type::Boolean(token),
            _ => return Err(Error::new(ErrorKind::Other,"TypeLiteral::from_token: Unexpected token kind: {:?}"))
        };
        Ok(r)
    }
}
