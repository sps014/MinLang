use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, TypeLiteral, ProgramNode, StatementNode};
use crate::lang::code_analysis::text::text_span::TextSpan;
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

    fn analyze_body(&self, body:&Vec<StatementNode>, parent_function:&FunctionNode,
                    parent_table:Option<&Rc<RefCell<SymbolTable>>>) ->Result<(),Error> {

        let parent_scope =match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope)));
        for statement in body.iter() {
            self.analyze_statement(statement,parent_function,&symbol_table)?;
        }
        dbg!(&symbol_table.borrow());
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
        let r=self.analyze_expression(right,parent_function,symbol_table)?;
        let l =(*symbol_table).as_ref().borrow().get_symbol(left.clone())?;
        self.compare_data_type(&l,&r,&left.position)?;
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
            ExpressionNode::Binary(left,opr,right)=>
                Ok(self.analyze_binary_expression(left,opr,right,parent_function,symbol_table)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table)?),
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented expression {:?}",expression)))
        };
    }
    fn analyze_binary_expression(&self,left:&ExpressionNode,opr:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode,
                                 symbol_table:&Rc<RefCell<SymbolTable>>)->Result<TypeLiteral,Error> {
        let left_value = self.analyze_expression(left,parent_function,symbol_table)?;
        let right_value = self.analyze_expression(right,parent_function,symbol_table)?;
        self.compare_data_type(&left_value,&right_value,&opr.position)?;
        return Ok(left_value);
    }
    fn compare_data_type(&self,left:&TypeLiteral,right:&TypeLiteral,position:&TextSpan)->Result<(),Error> {
        if left.get_type()==right.get_type()
        {
            return Ok(())
        }
        Err(Error::new(ErrorKind::Other,
                       format!("cannot convert from {} to {} at {}",
                       left.get_type(),right.get_type(),position.get_point_str())))
    }
    fn analyze_identifier(&self,id:&SyntaxToken,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<TypeLiteral,Error> {
        let r=(*symbol_table).as_ref().borrow_mut().get_symbol(id.clone())?;
        Ok(r)
    }

    fn analyze_if_else(&self, condition:&ExpressionNode, if_body:&Vec<StatementNode>,
                       else_if:&Vec<(ExpressionNode, Vec<StatementNode>)>,
                       else_body: &Option<Vec<StatementNode>>,
                       parent_function:&FunctionNode, symbol_table:&Rc<RefCell<SymbolTable>>) ->
    Result<(),Error>
    {
        //if condition
        self.analyze_expression(condition,parent_function,symbol_table)?;
        //if body
        self.analyze_body(if_body,parent_function,Some(symbol_table))?;

        //else if block
        for i in else_if.iter()
        {
            self.analyze_expression(&i.0,parent_function,symbol_table)?;
            self.analyze_body(&i.1,parent_function,Some(symbol_table))?;
        }
        match else_body
        {
            Some(body)=>self.analyze_body(body,parent_function,Some(symbol_table))?,
            None=>()
        }
        Ok(())
    }

}