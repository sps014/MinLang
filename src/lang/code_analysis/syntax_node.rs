use super::syntax_token::SyntaxToken;

#[derive(Debug)]
pub enum SyntaxNode {
    NumberExpressionSyntax(SyntaxToken),
    BinaryExpressionSyntax(Box<SyntaxNode>, SyntaxToken, Box<SyntaxNode>),
    ParenthesizedExpressionSyntax(SyntaxToken, Box<SyntaxNode>, SyntaxToken),
}

impl SyntaxNode {
    pub fn get_children(&self) -> Vec<SyntaxToken> {
        return match self {
            SyntaxNode::NumberExpressionSyntax(n) => vec![n.clone()],
            SyntaxNode::BinaryExpressionSyntax(left, opr, right) => {
                let mut v = vec![];
                for i in left.get_children() {
                    v.push(i);
                }
                v.push(opr.clone());
                for i in right.get_children() {
                    v.push(i);
                }
                v
            }
            SyntaxNode::ParenthesizedExpressionSyntax(open, expr, close) => {
                let mut v = vec![];
                v.push(open.clone());
                for i in expr.get_children() {
                    v.push(i);
                }
                v.push(close.clone());
                v
            }
            _ => vec![],
        };
    }
}
