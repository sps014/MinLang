use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::Parser;

pub struct WasmGenerator<'a>
{
    syntax_tree:&'a SyntaxTree
}
impl<'a> WasmGenerator<'a>
{
    pub fn new (syntax_tree:&'a SyntaxTree) -> Self
    {
        Self
        {
            syntax_tree
        }
    }
}