use crate::lang::code_analysis::syntax::syntax_node::ProgramNode;

pub struct SyntaxTree {
    root:ProgramNode,
}
impl SyntaxTree {
    pub fn new(root:ProgramNode) -> SyntaxTree {
        SyntaxTree {
            root,
        }
    }
    pub fn get_root(&self) -> ProgramNode {
        self.root.clone()
    }
}