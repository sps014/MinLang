use std::borrow::Borrow;
use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
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
        let ast = self.parser.parse()?;
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
        self.analyze_body(&function.body,function,None)?;
        Ok(())
    }
    fn analyze_body(&self, body:&Vec<StatementNode>, parent_function:&FunctionNode, parent_table:Option<Rc<RefCell<SymbolTable>>>) ->Result<(),Error> {


        let mut symbol_table =
            Rc::new(RefCell::new(SymbolTable::new(parent_table)));
        for statement in body.iter() {
            self.analyze_statement(statement,parent_function,&symbol_table)?;
        }
        Ok(())
    }
    fn analyze_statement(&self,statement:&StatementNode,parent_function:&FunctionNode,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error>
    {
        match statement
        {
            StatementNode::Declaration(left,right) =>
                self.analyze_declaration(left,right,parent_function,&symbol_table)?,
            StatementNode::Assignment(left,right) =>
                self.analyze_assignment(left,right,parent_function,&symbol_table)?,
            StatementNode::IfElse(condition,if_body,
                                  else_if,else_body)=>
                self.analyze_if_else(condition,if_body,
                                     else_if,else_body,parent_function,&symbol_table)?,
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented statement {:?}",statement)))
        };
        Ok(())
    }
    ///return type is returned currently int and float supported
    fn analyze_declaration(&self,left:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode,
                           symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error> {
        //return right type
        let right=self.analyze_expression(right,parent_function,symbol_table)?;
        (*symbol_table).as_ref().borrow_mut().add_symbol(left.text.clone(),right.clone())?;
        Ok(())
    }
    fn analyze_assignment(&self,left:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode,
                          symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error> {
        let right=self.analyze_expression(right,parent_function,symbol_table)?;
        let left =(*symbol_table).as_ref().borrow().get_symbol(left.text.clone())?;
        self.compare_data_type(&left,&right)?;
        Ok(())
    }
    fn analyze_expression(&self,expression:&ExpressionNode,parent_function:&FunctionNode,
                          symbol_table:&Rc<RefCell<SymbolTable>>)->Result<TypeLiteral,Error> {
        return match expression
        {
            ExpressionNode::Number(number) =>
                Ok(number.clone()),
            ExpressionNode::Unary(_,right)=>
                Ok(self.analyze_expression(right,parent_function,symbol_table)?),
            ExpressionNode::Binary(left,_,right)=>
                Ok(self.analyze_binary_expression(left,right,parent_function,symbol_table)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table)?),
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented expression {:?}",expression)))
        };
    }
    fn analyze_binary_expression(&self,left:&ExpressionNode,right:&ExpressionNode,parent_function:&FunctionNode,
                                 symbol_table:&Rc<RefCell<SymbolTable>>)->Result<TypeLiteral,Error> {
        let left_value = self.analyze_expression(left,parent_function,symbol_table)?;
        let right_value = self.analyze_expression(right,parent_function,symbol_table)?;
        self.compare_data_type(&left_value,&right_value)?;
        return Ok(left_value);
    }
    fn compare_data_type(&self,left:&TypeLiteral,right:&TypeLiteral)->Result<(),Error> {
        if left.get_type()==right.get_type()
        {
            return Ok(())
        }
        Err(Error::new(ErrorKind::Other,
                       format!("Binary expression {} and {} are not same type at {} and {}",
                       left.get_type(),right.get_type(),left.get_line_str(),right.get_line_str())))
    }
    fn analyze_identifier(&self,id:&SyntaxToken,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<TypeLiteral,Error> {
        let r=(*symbol_table).as_ref().borrow_mut().get_symbol(id.text.clone())?;
        Ok(r)
    }
    fn analyze_block(&self, body:&Vec<StatementNode>, parent_function:&FunctionNode,
                     parent_table: &Rc<RefCell<SymbolTable>>) ->Result<(),Error> {


        let mut symbol_table = Rc::new(
            RefCell::new(SymbolTable::new(Some(Rc::clone(parent_table)))));
        for statement in body.iter() {
            self.analyze_statement(statement,parent_function,&mut symbol_table)?;
        }
        Ok(())
    }
    fn analyze_if_else(&self, condition:&ExpressionNode, if_body:&Vec<StatementNode>,
                       else_if:&Vec<(ExpressionNode, Vec<StatementNode>)>,
                       else_body: &Option<Vec<StatementNode>>,
                       parent_function:&FunctionNode, symbol_table:&Rc<RefCell<SymbolTable>>) ->
    Result<(),Error>
    {
        let r=self.analyze_block(if_body,parent_function,symbol_table)?;
        Ok(())
    }

}