use crate::lang::code_analysis::syntax_token::SyntaxToken;

use super::syntax_node::SyntaxNode;

pub struct SyntaxTree {
    root: Box<SyntaxNode>,
    diagnostics: Vec<String>,
    eof_token: SyntaxToken,
}
impl SyntaxTree {
    pub fn new(
        diagnostics: Vec<String>,
        root: Box<SyntaxNode>,
        eof_token: SyntaxToken,
    ) -> SyntaxTree {
        SyntaxTree {
            root,
            diagnostics,
            eof_token,
        }
    }
}
