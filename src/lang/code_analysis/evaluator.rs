use crate::lang::code_analysis::syntax_kind::*;
use crate::lang::code_analysis::syntax_node::SyntaxNode;
use std::io::{Error, ErrorKind};

pub struct Evaluator {
    root: SyntaxNode,
}
impl Evaluator {
    pub fn new(expression: SyntaxNode) -> Evaluator {
        Evaluator { root: expression }
    }
    pub fn evaluate(&self) -> Result<i32, Error> {
        self.eval(&self.root)
    }
    fn eval(&self, node: &SyntaxNode) -> Result<i32, Error> {
        match node {
            SyntaxNode::NumberExpressionSyntax(token) => {
                return match token.text.parse::<i32>() {
                    Ok(n) => Result::Ok(n),
                    Err(e) => Err(Error::new(ErrorKind::Other, e.to_string())),
                }
            }
            SyntaxNode::BinaryExpressionSyntax(left, optr, right) => {
                let l = match self.eval(left) {
                    Ok(n) => n,
                    Err(_) => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("expected a number at {}",optr.position),
                        ))
                    }
                };
                let r = match self.eval(right) {
                    Ok(n) => n,
                    Err(_) => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("expected a number at {}",optr.position),
                        ))
                    }
                };
                return match optr.kind {
                    SyntaxKind::PlusToken => Ok(l + r),
                    SyntaxKind::MinusToken => Ok(l - r),
                    SyntaxKind::SlashToken => Ok(l / r),
                    SyntaxKind::StarToken => Ok(l * r),
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("Error unexpected kind {:?}", optr.kind),
                        ))
                    }
                };
            }
            SyntaxNode::ParenthesizedExpressionSyntax(_, ex, _) => {
                return self.eval(ex);
            }
            _ => {
                return Result::Err(Error::new(
                    ErrorKind::Other,
                    format!("Error cant parse expression invalid node {:?}", node),
                ));
            }
        }
    }
}
