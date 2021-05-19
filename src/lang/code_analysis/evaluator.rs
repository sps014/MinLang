use crate::lang::code_analysis::syntax_kind::*;
use crate::lang::code_analysis::syntax_node::SyntaxNode;
use std::{
    io::{Error, ErrorKind},
    panic::resume_unwind,
};

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
                            format!("expected a number at {}", optr.position),
                        ))
                    }
                };
                let r = match self.eval(right) {
                    Ok(n) => n,
                    Err(_) => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("expected a number at {}", optr.position),
                        ))
                    }
                };
                return match optr.kind {
                    SyntaxKind::PlusToken => Ok(l + r),
                    SyntaxKind::MinusToken => Ok(l - r),
                    SyntaxKind::SlashToken => Ok(l / r),
                    SyntaxKind::StarToken => Ok(l * r),
                    SyntaxKind::BitWiseAmpersandToken => Ok(l & r),
                    SyntaxKind::BitWisePipeToken => Ok(l | r),

                    _ => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("Error unexpected kind {:?} at {}", optr.kind, optr.position),
                        ))
                    }
                };
            }
            SyntaxNode::ParenthesizedExpressionSyntax(_, ex, _) => {
                return self.eval(ex);
            }
            SyntaxNode::UnaryExpressionSyntax(op, exp) => match op.kind {
                SyntaxKind::PlusToken => {
                    return self.eval(exp);
                }
                SyntaxKind::MinusToken => {
                    let v = self.eval(exp);
                    return match v {
                        Ok(n) => Ok(-n),
                        Err(e) => Err(Error::new(
                            ErrorKind::Other,
                            format!("Error invalid unary expression {:?}", node),
                        )),
                    };
                }
                _ => {
                    return Result::Err(Error::new(
                        ErrorKind::Other,
                        format!("Error invalid unary expression {:?}", node),
                    ));
                }
            },
            _ => {
                return Result::Err(Error::new(
                    ErrorKind::Other,
                    format!("Error cant parse expression invalid node {:?}", node),
                ));
            }
        }
    }
}
