use std::fmt::Debug;
use std::hash::Hash;

pub trait SyntaxNode
{
   fn get_parent(&self) -> Option<&dyn SyntaxNode>;
}

pub struct ProgramNode<'a>
{
    pub functions: Vec<FunctionNode<'a>>,
}
impl<'b> SyntaxNode for ProgramNode<'b>
{
    fn get_parent(&self) -> Option<&dyn SyntaxNode>
    {
        None
    }
}

impl<'ab> ProgramNode<'ab> {
    pub fn new(functions:Vec<FunctionNode<'ab>>) -> ProgramNode<'ab> {
        ProgramNode {
            functions
        }
    }
}

pub struct FunctionNode<'a>
{
    pub name: String,
    pub parameters: Vec<ParameterNode>,
    pub body: Vec<StatementNode>,
    parent: Option<&'a dyn SyntaxNode>,
}
impl<'a> SyntaxNode for FunctionNode<'a>
{
    fn get_parent(&self) -> Option<&dyn SyntaxNode>
    {
        self.parent.clone()
    }
}

pub struct ParameterNode
{
    pub name: String,
    pub type_: String,
}

pub enum StatementNode
{
    Assignment(String, ExpressionNode),
    Return(Option<ExpressionNode>),
}
pub enum ExpressionNode
{
    NumberLiteral,
    StringLiteral(String),
    Identifier(String),
    FunctionCall(String, Vec<ExpressionNode>),
}
pub enum NumberLiteral
{
    Integer(i32),
    Float(f32),
}