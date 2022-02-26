use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, Type, ProgramNode, StatementNode};
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::semantic_analysis::function_control_flow::FunctionControlGraph;
use crate::lang::semantic_analysis::function_table::{FunctionTable, FunctionTableInfo};
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use crate::Parser;

pub struct Anaylzer<'a> {
    parser: Parser<'a>,
    function_table:FunctionTable
}
impl<'a> Anaylzer<'a> {
    pub fn new(parser: Parser<'a>) -> Self {
        Self { parser, function_table: FunctionTable::new() }
    }
    pub fn analyze(&mut self) -> Result<(), Error> {
        let ast = self.parser.parse()?;
        let pgm= ast.get_root();
        self.analyze_pgm(pgm.clone())?;
        Ok(())
    }
    fn analyze_pgm(&mut self,node:ProgramNode) -> Result<(), Error> {
     for function in node.functions.iter() {
         self.analyze_function(function)?;
     }
        Ok(())
    }
    fn analyze_function(&mut self,function:&FunctionNode) -> Result<(), Error> {
        self.analyze_body(&function.body,function,None,false)?;
        // check return
        let mut graph=FunctionControlGraph::new(function);
        //graph.build()?;
        let cp=function.clone();
        self.function_table.add_function(cp.name.text,FunctionTableInfo::from(function))?;
        Ok(())
    }

    fn analyze_body(&self, body:&Vec<StatementNode>, parent_function:&FunctionNode,
                    parent_table:Option<&Rc<RefCell<SymbolTable>>>,has_parent_loop:bool) ->Result<(),Error> {

        let parent_scope =match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope)));
        for statement in body.iter() {
            self.analyze_statement(statement,parent_function,&symbol_table,has_parent_loop)?;
        }
        Ok(())
    }
    fn analyze_statement(&self,statement:&StatementNode,parent_function:&FunctionNode,
                         symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool)->Result<(),Error>
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
                                     else_if,else_body,parent_function,&symbol_table,has_parent_while)?,
            StatementNode::Return(expression) =>
                self.analyze_return(expression,parent_function,&symbol_table)?,
            StatementNode::While(condition,body) =>
                self.analyze_while(condition,body,parent_function,&symbol_table)?,
            StatementNode::Break=>
                self.analyze_break(parent_function,&symbol_table,has_parent_while)?,
            StatementNode::Continue=>
                self.analyze_continue(parent_function,&symbol_table,has_parent_while)?,
            StatementNode::FunctionInvocation(name,params) =>
                {self.analyze_function_call(name,params,parent_function,symbol_table)?;},
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented statement {:?}",statement)))
        };
        Ok(())
    }
    fn analyze_function_call(&self,name:&SyntaxToken,params:&Vec<ExpressionNode>,
                                   parent_function:&FunctionNode,
                                   symbol_table:&Rc<RefCell<SymbolTable>>)->Result<Type,Error> {
        let function_name=name.text.clone();
        let mut params_types=vec![];
        for param in params.iter() {
            params_types.push(self.analyze_expression(param,parent_function,symbol_table)?.get_type());
        }
        let store_sig=self.function_table.get_function(&function_name)?;

        if store_sig.parameters.len()!=params_types.len() {
            return Err(Error::new(ErrorKind::Other,format!("Function {} has {} params but {} params are given",
                                                           function_name,store_sig.parameters.len(),params_types.len())));
        }

        for i in 0..params_types.len() {
            if store_sig.parameters.get(i)!=params_types.get(i) {
                return Err(Error::new(ErrorKind::Other,format!("Function {} has param {} of type {:?} but param {} of type {:?} is given",
                                                               function_name,i,store_sig.parameters.get(i),i,params_types[i])));
            }
        }

        //let r_type=&store_sig.return_type;
        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }
    fn analyze_break(&self,parent_function:&FunctionNode,symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool)->Result<(),Error> {
        if !has_parent_while {
            return Err(Error::new(ErrorKind::Other,
                                  format!("Break statement is not in a while loop in function {}",parent_function.name.text)));
        }
        Ok(())
    }
    fn analyze_continue(&self,parent_function:&FunctionNode,symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool)->Result<(),Error> {
        if !has_parent_while {
            return Err(Error::new(ErrorKind::Other,
                                  format!("Continue statement is not in a while loop in function {}",parent_function.name.text)));
        }
        Ok(())
    }
    fn analyze_while(&self,condition:&ExpressionNode,body:&Vec<StatementNode>,
                     parent_function:&FunctionNode,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error>
    {
        self.analyze_expression(condition,parent_function,symbol_table)?;
        self.analyze_body(body,parent_function,Some(symbol_table),true)?;
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
                          symbol_table:&Rc<RefCell<SymbolTable>>)->Result<Type,Error> {
        return match expression
        {
            ExpressionNode::Literal(number) =>
                Ok(number.clone()),
            ExpressionNode::Unary(_,right)=>
                Ok(self.analyze_expression(right,parent_function,symbol_table)?),
            ExpressionNode::Binary(left,opr,right)=>
                Ok(self.analyze_binary_expression(left,opr,right,parent_function,symbol_table)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table)?),
            ExpressionNode::FunctionCall(name,params)=>
                Ok(self.analyze_function_call(name,params,parent_function,symbol_table)?),
            ExpressionNode::Parenthesized(expr)=>
                Ok(self.analyze_expression(expr,parent_function,symbol_table)?),
            _=>return Err(Error::new(ErrorKind::Other,format!("Not implemented expression {:?}",expression)))
        };
    }
    fn analyze_binary_expression(&self,left:&ExpressionNode,opr:&SyntaxToken,right:&ExpressionNode,parent_function:&FunctionNode,
                                 symbol_table:&Rc<RefCell<SymbolTable>>)->Result<Type,Error> {
        let left_value = self.analyze_expression(left,parent_function,symbol_table)?;
        let right_value = self.analyze_expression(right,parent_function,symbol_table)?;
        self.compare_data_type(&left_value,&right_value,&opr.position)?;
        match (&left_value,&opr.kind) {
          (Type::String(_),TokenKind::PlusToken)=> {}
          (Type::String(_),_)=>
              return Err(Error::new(ErrorKind::Other,format!("Cannot perform operation {} on string",opr.text))),
            (_,_)=>{}
        };
        return Ok(left_value);
    }
    fn compare_data_type(&self, left:&Type, right:&Type, position:&TextSpan) ->Result<(),Error> {
        if left.get_type()==right.get_type()
        {
            return Ok(())
        }
        Err(Error::new(ErrorKind::Other,
                       format!("cannot convert from {} to {} at {}",
                       left.get_type(),right.get_type(),position.get_point_str())))
    }
    fn analyze_identifier(&self,id:&SyntaxToken,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<Type,Error> {
        let r=(*symbol_table).as_ref().borrow_mut().get_symbol(id.clone())?;
        Ok(r)
    }

    fn analyze_if_else(&self, condition:&ExpressionNode, if_body:&Vec<StatementNode>,
                       else_if:&Vec<(ExpressionNode, Vec<StatementNode>)>,
                       else_body: &Option<Vec<StatementNode>>,
                       parent_function:&FunctionNode, symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool) ->
    Result<(),Error>
    {
        //if condition
        self.analyze_expression(condition,parent_function,symbol_table)?;
        //if body
        self.analyze_body(if_body,parent_function,Some(symbol_table),has_parent_while)?;

        //else if block
        for i in else_if.iter()
        {
            self.analyze_expression(&i.0,parent_function,symbol_table)?;
            self.analyze_body(&i.1,parent_function,Some(symbol_table),has_parent_while)?;
        }
        match else_body
        {
            Some(body)=>self.analyze_body(body,parent_function,Some(symbol_table),has_parent_while)?,
            None=>()
        }
        Ok(())
    }
    fn analyze_return(&self,expression:&Option<ExpressionNode>,parent_function:&FunctionNode,
                      symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error> {
        match (expression,&parent_function.return_type)
        {
            (Some(expression),&Some(ref return_type))=>
            {
                let r=self.analyze_expression(expression,parent_function,symbol_table)?;
                if r.get_type()==return_type.get_type() {
                    return Ok(())
                }
                return Err(Error::new(ErrorKind::Other,
                                      format!("cannot convert return to {} from {}",
                                              r.get_type(),return_type.get_type())))
            },
            (None,&Some(_))=>
                return Err(Error::new(ErrorKind::Other,format!("return type mismatch at  {}",parent_function.name.position.get_point_str()))),
            (Some(_),&None)=>
                return Err(Error::new(ErrorKind::Other,format!("return type mismatch at {}",parent_function.name.position.get_point_str()))),
            (None,&None)=>()
        };
        Ok(())
    }

}