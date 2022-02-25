use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, TypeLiteral, ProgramNode, StatementNode};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
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
        self.analyze_pgm(pgm.clone())?;
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
       let mut symbol_table = SymbolTable::new();

        for statement in body.iter() {
            self.analyze_statement(statement,parent_function,&mut symbol_table)?;
        }
        Ok(())
    }
    fn analyze_statement(&self,statement:&StatementNode,parent_function:&FunctionNode,symbol_table:&mut SymbolTable)->Result<(),Error>
    {
        match statement
        {
            StatementNode::Declaration(left,right) =>
                self.analyze_declaration(left,right,parent_function,symbol_table)?,
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented statement {:?}",statement)))
        };
        Ok(())
    }
    ///return type is returned currently int and float supported
    fn analyze_declaration(&self,left:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode,symbol_table:&mut SymbolTable)->Result<(TypeLiteral),Error> {
        //return right type
        let right=self.analyze_expression(right,parent_function,symbol_table)?;
        symbol_table.add_symbol(left.text.clone(),right.clone());
        Ok(right)
    }
    fn analyze_expression(&self,expression:&ExpressionNode,parent_function:&FunctionNode,symbol_table:&mut SymbolTable)->Result<(TypeLiteral),Error> {
        return match expression
        {
            ExpressionNode::Number(number) =>
                Ok(number.clone()),
            ExpressionNode::Unary(op,right)=>
                Ok(self.analyze_expression(right,parent_function,symbol_table)?),
            ExpressionNode::Binary(left,op,right)=>
                Ok(self.analyze_binary_expression(left,op,right,parent_function,symbol_table)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table)?),
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented expression {:?}",expression)))
        };
    }
    fn analyze_binary_expression(&self,left:&ExpressionNode,op:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode,symbol_table:&mut SymbolTable)->Result<(TypeLiteral),Error> {
        let left_value = self.analyze_expression(left,parent_function,symbol_table)?;
        let right_value = self.analyze_expression(right,parent_function,symbol_table)?;
        return match (&left_value,&right_value) {
            (TypeLiteral::Float(_), TypeLiteral::Float(_))=>
                 Ok(left_value),
            (TypeLiteral::Integer(_), TypeLiteral::Integer(_))=>
                 Ok(left_value),
            (TypeLiteral::String(_), TypeLiteral::String(_))=>
                 Ok(left_value),
            _=>
                 Err(Error::new(ErrorKind::Other,format!("Binary expression {:?} and {:?} are not same type",left_value,right_value)))
        }
    }
    fn analyze_identifier(&self,id:&SyntaxToken,symbol_table:&mut SymbolTable)->Result<(TypeLiteral),Error> {
        symbol_table.get_symbol(id.text.clone())
    }
}