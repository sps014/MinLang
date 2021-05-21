use crate::lang::code_analysis::syntax_kind::*;
use crate::lang::code_analysis::syntax_node::SyntaxNode;
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub struct Evaluator {
    root: SyntaxNode,
}
impl Evaluator {
    pub fn new(expression: SyntaxNode) -> Evaluator {
        Evaluator { root: expression }
    }
    pub fn evaluate(&mut self, variables: &mut HashMap<String, i32>) -> Result<i32, Error> {
        self.eval(&self.root.clone(), variables)
    }
    fn eval(
        &mut self,
        node: &SyntaxNode,
        variables: &mut HashMap<String, i32>,
    ) -> Result<i32, Error> {
        match node {
            SyntaxNode::FunctionCallExpression(id, open, expr, close) => {
                if id.kind != SyntaxKind::IdentifierToken
                    || open.kind != SyntaxKind::OpenParenthesisToken
                    || close.kind != SyntaxKind::CloseParenthesisToken
                {
                    return Err(Error::new(
                        ErrorKind::Other,
                        format!("error in function call"),
                    ));
                }
                for i in expr {
                    let g = self.eval(i.as_ref(), variables);
                    match g {
                        Err(e) => {
                            return Err(Error::new(ErrorKind::Other, format!("{:?}", e)));
                        }
                        Ok(v) => {
                            if v!=i32::MAX{
                            println!("{}", v);
                            }
                        }
                    }
                }
                return Ok(i32::MAX);
            }
            SyntaxNode::WhileLoopSyntax(whi, cond, body) => {
                if whi.text != "while" {
                    return Err(Error::new(
                        ErrorKind::Other,
                        format!("invalid while loop {}", whi.position),
                    ));
                }

                while match self.eval(cond, variables) {
                    Ok(cond) => cond != 0,
                    Err(e) => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("Error in while condition"),
                        ));
                    }
                } {
                    let body = self.eval(body, variables);
                    match body {
                        Err(e) => {
                            return Err(Error::new(ErrorKind::Other, format!("{:?}", e)));
                        }
                        Ok(e) => {},
                    }
                }
                return Ok(i32::MAX);
            }
            SyntaxNode::IfBlockSyntax(ifb, cond, body) => {
                if ifb.text != "if" {
                    return Err(Error::new(
                        ErrorKind::Other,
                        format!("invalid if syntax {}", ifb.position),
                    ));
                }

                if match self.eval(cond, variables) {
                    Ok(cond) => cond != 0,
                    Err(e) => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("Error in while condition"),
                        ));
                    }
                } {
                    let body = self.eval(body, variables);
                    match body {
                        Err(e) => {
                            return Err(Error::new(ErrorKind::Other, format!("{:?}", e)));
                        }
                        Ok(e) =>{}
                    }
                }
                return Ok(i32::MAX);
            }
            SyntaxNode::BlockExpressionSyntax(open, exps, close) => {
                if open.kind != SyntaxKind::CurlyOpenBracketToken
                    || close.kind != SyntaxKind::CurlyCloseBracketToken
                {
                    return Err(Error::new(
                        ErrorKind::Other,
                        format!("mismatch in curly braces {}", open.position),
                    ));
                }
                let mut last = 0;
                for i in exps {
                    last = match self.eval(i.as_ref(), variables) {
                        Ok(j) => j,
                        Err(e) => {
                            return Err(Error::new(
                                ErrorKind::Other,
                                format!(" {} at {}", e, open.position),
                            ));
                        }
                    }
                }
                return Ok(i32::MAX);
            }
            SyntaxNode::AssignmentExpressionSyntax(id, op, expr) => {
                let r = self.eval(expr, variables);
                match r {
                    Ok(r) => {
                        variables.insert(id.text.clone(), r);
                        return Ok(r);
                    }
                    Err(e) => {
                        return Err(Error::new(ErrorKind::Other, e.to_string()));
                    }
                }
            }
            SyntaxNode::NumberExpressionSyntax(token) => {
                if token.kind == SyntaxKind::IdentifierToken {
                    if variables.contains_key(&token.text.clone()) {
                        return Ok(variables[&token.text]);
                    } else {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("undefined var {}", token.text),
                        ));
                    }
                }
                return match token.text.parse::<i32>() {
                    Ok(n) => Result::Ok(n),
                    Err(e) => Err(Error::new(ErrorKind::Other, e.to_string())),
                };
            }
            SyntaxNode::BinaryExpressionSyntax(left, optr, right) => {
                let l = match self.eval(left, variables) {
                    Ok(n) => n,
                    Err(_) => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("expected a number at {}", optr.position),
                        ))
                    }
                };
                let r = match self.eval(right, variables) {
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
                    SyntaxKind::GreaterThanEqualToken => Ok((l >= r) as i32),
                    SyntaxKind::GreaterThanToken => Ok((l > r) as i32),
                    SyntaxKind::SmallerThanEqualToken => Ok((l <= r) as i32),
                    SyntaxKind::SmallerThanToken => Ok((l < r) as i32),
                    SyntaxKind::EqualEqualToken => Ok((l == r) as i32),
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("Error unexpected kind {:?} at {}", optr.kind, optr.position),
                        ))
                    }
                };
            }
            SyntaxNode::ParenthesizedExpressionSyntax(_, ex, _) => {
                return self.eval(ex, variables);
            }
            SyntaxNode::UnaryExpressionSyntax(op, exp) => match op.kind {
                SyntaxKind::PlusToken => {
                    return self.eval(exp, variables);
                }
                SyntaxKind::MinusToken => {
                    let v = self.eval(exp, variables);
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
