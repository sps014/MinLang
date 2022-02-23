use std::fmt::Debug;
use std::hash::Hash;

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
    pub name: String,
    pub return_type: String,
    pub parameters: Vec<ParameterNode>,
    pub body: Vec<StatementNode>,
}

impl FunctionNode {
    pub fn new(name: String, return_type: String, parameters: Vec<ParameterNode>, body: Vec<StatementNode>) -> FunctionNode {
        FunctionNode { name, return_type, parameters, body }
    }
}

#[derive(Debug)]
pub struct ParameterNode
{
    pub name: String,
    pub type_: String,
}
impl ParameterNode
{
    pub fn new(name: String, type_: String) -> ParameterNode {
        ParameterNode { name, type_ }
    }
}

#[derive(Debug,Clone)]
pub enum StatementNode
{
    Assignment(String, ExpressionNode),
    Return(Option<ExpressionNode>),
}

#[derive(Debug,Clone)]
pub enum ExpressionNode
{
    NumberLiteral,
    StringLiteral(String),
    Identifier(String),
    FunctionCall(String, Vec<ExpressionNode>),
}

#[derive(Debug,Clone)]
pub enum NumberLiteral
{
    Integer(i32),
    Float(f32),
}