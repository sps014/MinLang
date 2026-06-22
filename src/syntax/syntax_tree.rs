use crate::lang::code_analysis::syntax::nodes::ProgramNode;

pub struct SyntaxTree<'a> {
    root:ProgramNode<'a>,
}
impl<'a> SyntaxTree<'a> {
    pub fn new(root:ProgramNode<'a>) -> SyntaxTree<'a> {
        SyntaxTree {
            root,
        }
    }
    pub fn get_root(&self) -> &ProgramNode<'a> {
        &self.root
    }
}