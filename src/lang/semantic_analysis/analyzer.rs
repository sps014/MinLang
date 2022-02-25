use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, NumberLiteral, ProgramNode, StatementNode};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::Parser;

pub struct Anaylzer<'a> {
    parser: Parser<'a>,
}
impl<'a> Anaylzer<'a> {
    pub fn new(parser: Parser<'a>) -> Self {
        Self { parser }
    }
    pub fn analyze(&mut self) -> Result<(), Error> {
        let mut ast = self.parser.parse()?;
        let pgm= ast.get_root();
        self.analyze_pgm(pgm.clone());
        Ok(())
    }
    fn analyze_pgm(&self,node:ProgramNode) -> Result<(), Error> {
     for function in node.functions.iter() {
         self.analyze_function(function)?;
     }
        Ok(())
    }
    fn analyze_function(&self,function:&FunctionNode) -> Result<(), Error> {
        self.analyze_body(&function.body,function)?;
        Ok(())
    }
    fn analyze_body(&self,body:&Vec<StatementNode>,parent_function:&FunctionNode)->Result<(),Error> {
        for statement in body.iter() {
            self.analyze_statement(statement,parent_function)?;
        }
        Ok(())
    }
    fn analyze_statement(&self,statement:&StatementNode,parent_function:&FunctionNode)->Result<(),Error>
    {
        match statement
        {
            StatementNode::Declaration(left,right) =>
                self.analyze_declaration(left,right,parent_function)?,
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented statement {:?}",statement)))
        };
        Ok(())
    }
    ///return type is returned currently int and float supported
    fn analyze_declaration(&self,left:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode)->Result<(NumberLiteral),Error> {
        //return right type
        Ok(self.analyze_expression(right,parent_function)?)
    }
    fn analyze_expression(&self,expression:&ExpressionNode,parent_function:&FunctionNode)->Result<(NumberLiteral),Error> {
        match expression
        {
            ExpressionNode::Number(number) =>
               return  Ok(number.clone()),
            ExpressionNode::Unary(op,right)=>
            return Ok(self.analyze_expression(right,parent_function)?),
            ExpressionNode::Binary(left,op,right)=>
            return Ok(self.analyze_binary_expression(left,op,right,parent_function)?),
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented expression {:?}",expression)))
        };
    }
    fn analyze_binary_expression(&self,left:&ExpressionNode,op:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode)->Result<(NumberLiteral),Error> {
        let left_value = self.analyze_expression(left,parent_function)?;
        let right_value = self.analyze_expression(right,parent_function)?;
        if left_value==right_value {
            return Ok(left_value);
        }
        Err(Error::new(ErrorKind::Other,format!("Binary expression {:?} and {:?} are not same type",left_value,right_value)))
    }
}