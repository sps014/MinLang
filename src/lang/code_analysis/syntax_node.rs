use std::vec;


// #[derive(Debug, Clone)]
// pub enum SyntaxNode {
//     StatementsSyntax(Vec<Box<SyntaxNode>>),
//     NumberExpressionSyntax(SyntaxToken),
//     BinaryExpressionSyntax(Box<SyntaxNode>, SyntaxToken, Box<SyntaxNode>),
//     ParenthesizedExpressionSyntax(SyntaxToken, Box<SyntaxNode>, SyntaxToken),
//     UnaryExpressionSyntax(SyntaxToken, Box<SyntaxNode>),
//     AssignmentExpressionSyntax(SyntaxToken, SyntaxToken, Box<SyntaxNode>),
//     FunctionCallExpression(SyntaxToken, SyntaxToken, Vec<Box<SyntaxNode>>, SyntaxToken),
//     BlockExpressionSyntax(SyntaxToken, Vec<Box<SyntaxNode>>, SyntaxToken),
//     WhileLoopSyntax(SyntaxToken, Box<SyntaxNode>, Box<SyntaxNode>),
//     IfBlockSyntax(SyntaxToken, Box<SyntaxNode>,Box<SyntaxNode>),
// }
// enum SyntaxCol {
//     Token(SyntaxToken),
//     Node(SyntaxNode),
// }
//
// impl SyntaxNode {
//     pub fn get_children(&self) -> Vec<SyntaxCol> {
//         return match self {
//             SyntaxNode::NumberExpressionSyntax(n) => vec![SyntaxCol::Token(n.clone())],
//             SyntaxNode::BinaryExpressionSyntax(left, opr, right) => {
//                 vec![
//                     SyntaxCol::Node(left.as_ref().clone()),
//                     SyntaxCol::Token(opr.clone()),
//                     SyntaxCol::Node(right.as_ref().clone()),
//                 ]
//             }
//             SyntaxNode::ParenthesizedExpressionSyntax(open, expr, close) => {
//                 vec![
//                     SyntaxCol::Token(open.clone()),
//                     SyntaxCol::Node(expr.as_ref().clone()),
//                     SyntaxCol::Token(close.clone()),
//                 ]
//             }
//             _ => vec![],
//         };
//     }
// }
