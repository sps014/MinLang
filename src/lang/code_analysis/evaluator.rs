use crate::lang::code_analysis::syntax_node::SyntaxNode;
use crate::lang::code_analysis::syntax_kind::*;
use std::io::{Error, ErrorKind};

pub struct Evaluator
{
    root:SyntaxNode,
}
impl Evaluator
{
    pub fn new(expression: SyntaxNode) -> Evaluator
    {
        Evaluator { root: expression }
    }
    pub fn evaluate(&self) -> Result<i32, Error>
    {
        self.eval(&self.root)
    }
    fn eval(&self,node:&SyntaxNode) -> Result<i32, Error>
    {
        match node {
            SyntaxNode::NumberExpressionSyntax(token)=>
                {
                    return  Result::Ok(token.text.parse::<i32>().unwrap());
                },
            SyntaxNode::BinaryExpressionSyntax(left,optr,right)=>
                {
                    let l=self.eval(left).unwrap();
                    let r=self.eval(right).unwrap();
                    let mut res=0;
                    match optr.kind {
                       
                        SyntaxKind::PlusToken => { res=l+r;}
                        SyntaxKind::MinusToken => {res=l-r;}
                        SyntaxKind::SlashToken => {res=l/r;}
                        SyntaxKind::StarToken => {res=l*r;}
                        _=>{
                         return Result::Err(Error::new(ErrorKind::Other,format!("Error unexpected kind {:?}",optr.kind)));
                        }
                        
                    }
                    return  Result::Ok(res);
                },
            SyntaxNode::ParenthesizedExpressionSyntax(left,ex,right)=>
                {
                  return  self.eval(ex);
                },
            _=> {
                return Result::Err(Error::new(ErrorKind::Other,format!("Error cant parse expression invalid node {:?}",node)));
            }
        }

    }
}