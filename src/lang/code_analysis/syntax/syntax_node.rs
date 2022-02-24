use std::fmt::Debug;
use std::hash::Hash;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug)]
pub struct ProgramNode
{
    pub functions: Vec<FunctionNode>,
}

impl ProgramNode {
    pub fn new(functions: Vec<FunctionNode>) -> ProgramNode {
        ProgramNode { functions }
    }
}

#[derive(Debug)]
pub struct FunctionNode
{
    pub name: SyntaxToken,
    pub return_type: Option<SyntaxToken>,
    pub parameters: Vec<ParameterNode>,
    pub body: Vec<StatementNode>,
}

impl FunctionNode {
    pub fn new(name: SyntaxToken, return_type: Option<SyntaxToken>, parameters: Vec<ParameterNode>, body: Vec<StatementNode>) -> FunctionNode {
        FunctionNode { name, return_type, parameters, body }
    }
}

#[derive(Debug)]
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
}

#[derive(Debug,Clone)]
pub enum ExpressionNode
{
    Number(NumberLiteral),
    Binary(Box<ExpressionNode>, SyntaxToken, Box<ExpressionNode>),
    Unary(SyntaxToken, Box<ExpressionNode>),
    StringLiteral(SyntaxToken),
    Identifier(SyntaxToken),
    Parenthesized(Box<ExpressionNode>),
    FunctionCall(SyntaxToken, Vec<ExpressionNode>),
}

#[derive(Debug,Clone)]
pub enum NumberLiteral
{
    Integer(SyntaxToken),
    Float(SyntaxToken),
}