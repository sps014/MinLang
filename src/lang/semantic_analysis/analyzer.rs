use std::cell::{ RefCell};
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, Type, ProgramNode, StatementNode};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::semantic_analysis::function_control_flow::FunctionControlGraph;
use crate::lang::semantic_analysis::function_table::{FunctionTable, FunctionTableInfo};
use crate::lang::semantic_analysis::symbol_table::SymbolTable;

pub struct SemanticInfo<'a>
{
    pub hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
}

impl<'a> SemanticInfo<'a> {
    pub fn new(hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>, function_table: &FunctionTable) -> SemanticInfo<'_>
    {
        SemanticInfo {
            hash_map,
            function_table,
        }
    }
}


pub struct Anaylzer<'a> {
    syntax_tree:&'a SyntaxTree<'a> ,
    function_table:FunctionTable
}
impl<'a> Anaylzer<'a> {
    pub fn new(tree: &'a SyntaxTree<'a>) -> Self {
        Self { syntax_tree:tree, function_table: FunctionTable::new() }
    }
    pub fn analyze(&mut self) -> Result<SemanticInfo<'_>, Error> {
        let pgm= self.syntax_tree.get_root();
        self.analyze_pgm(pgm)
    }
    fn analyze_pgm(&mut self,node:&ProgramNode<'a>) -> Result<SemanticInfo<'_>, Error> {
        let mut symbol_table_map=HashMap::new();
        for function in node.functions.iter() {
         let r=self.analyze_function(function)?;
         symbol_table_map.insert(function.name.text.clone(),r);
     }
        Ok(SemanticInfo::new(symbol_table_map,&self.function_table))
    }
    fn analyze_function(&mut self,function:&FunctionNode<'a>) -> Result<Rc<RefCell<SymbolTable>>, Error> {
        let param_table=Rc::new(RefCell::new(self.add_function_param_table(function)?));
        self.analyze_body(function.body,function,Some(&param_table),false)?;
        // check return
        let mut graph=FunctionControlGraph::new(function);
        graph.build()?;
        self.function_table.add_function(function.name.text.clone(),FunctionTableInfo::from(function))?;
        Ok(param_table.clone())
    }
    fn add_function_param_table(&mut self,function:&FunctionNode<'a>) -> Result<SymbolTable, Error> {
        let mut param_table=SymbolTable::new(None);
        for param in function.parameters.iter() {
            param_table.add_symbol(param.name.text.clone(),Type::from_token(param.type_.clone())?)?;
        }
        Ok(param_table)
    }

    fn analyze_body(&self, body:&[StatementNode<'a>], parent_function:&FunctionNode<'a>,
                    parent_table:Option<&Rc<RefCell<SymbolTable>>>,has_parent_loop:bool) ->Result<(),Error> {

        let parent_scope =match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope.clone())));
        if parent_scope.is_some()
        {
            let parent_table=&parent_scope.unwrap();
            (*parent_table).borrow_mut().add_child(symbol_table.clone());
        }
        for statement in body.iter() {
            let clone=&symbol_table.clone();
            self.analyze_statement(statement,parent_function,&clone,has_parent_loop)?;
        }
        Ok(())
    }
    fn analyze_statement(&self,statement:&StatementNode<'a>,parent_function:&FunctionNode<'a>,
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
            StatementNode::For(init,condition,increment,body) =>
                self.analyze_for(init,condition,increment,body,parent_function,&symbol_table)?,
            StatementNode::Break=>
                self.analyze_break(parent_function,has_parent_while)?,
            StatementNode::Continue=>
                self.analyze_continue(parent_function,has_parent_while)?,
            StatementNode::FunctionInvocation(name,params) =>
                {self.analyze_function_call(name,params,parent_function,symbol_table)?;},
        };
        Ok(())
    }
    fn analyze_function_call(&self,name:&SyntaxToken,params:&Vec<ExpressionNode<'a>>,
                                   parent_function:&FunctionNode<'a>,
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
    fn analyze_break(&self,parent_function:&FunctionNode<'a>,has_parent_while:bool)->Result<(),Error> {
        if !has_parent_while {
            return Err(Error::new(ErrorKind::Other,
                                  format!("Break statement is not in a while loop in function {}",parent_function.name.text)));
        }
        Ok(())
    }
    fn analyze_continue(&self,parent_function:&FunctionNode<'a>,has_parent_while:bool)->Result<(),Error> {
        if !has_parent_while {
            return Err(Error::new(ErrorKind::Other,
                                  format!("Continue statement is not in a while loop in function {}",parent_function.name.text)));
        }
        Ok(())
    }
    fn analyze_while(&self,condition:&ExpressionNode<'a>,body:&[StatementNode<'a>],
                     parent_function:&FunctionNode<'a>,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error>
    {
        let cond_type = self.analyze_expression(condition,parent_function,symbol_table)?;
        if cond_type.get_type() != "bool" {
            return Err(Error::new(ErrorKind::Other, format!("while condition must be bool, got {}", cond_type.get_type())));
        }
        self.analyze_body(body,parent_function,Some(symbol_table),true)?;
        Ok(())
    }
    fn analyze_for(&self,init:&Option<&'a StatementNode<'a>>,condition:&Option<ExpressionNode<'a>>,
                   increment:&Option<&'a StatementNode<'a>>,body:&[StatementNode<'a>],
                   parent_function:&FunctionNode<'a>,symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error>
    {
        let for_scope = Rc::new(RefCell::new(SymbolTable::new(Some(symbol_table.clone()))));
        (*symbol_table).borrow_mut().add_child(for_scope.clone());

        if let Some(init_stmt) = init {
            self.analyze_statement(init_stmt, parent_function, &for_scope, false)?;
        }
        if let Some(cond_expr) = condition {
            let cond_type = self.analyze_expression(cond_expr, parent_function, &for_scope)?;
            if cond_type.get_type() != "bool" {
                return Err(Error::new(ErrorKind::Other, format!("for condition must be bool, got {}", cond_type.get_type())));
            }
        }
        if let Some(inc_stmt) = increment {
            self.analyze_statement(inc_stmt, parent_function, &for_scope, false)?;
        }
        self.analyze_body(body, parent_function, Some(&for_scope), true)?;
        Ok(())
    }
    ///return type is returned currently int and float supported
    fn analyze_declaration(&self,left:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                           symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error> {
        //return right type
        let right=self.analyze_expression(right,parent_function,symbol_table)?;
        (*symbol_table).as_ref().borrow_mut().add_symbol(left.text.clone(),right.clone())?;
        Ok(())
    }
    fn analyze_assignment(&self,left:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                          symbol_table:&Rc<RefCell<SymbolTable>>)->Result<(),Error> {
        let r=self.analyze_expression(right,parent_function,symbol_table)?;
        let l =(*symbol_table).as_ref().borrow().get_symbol(left.clone())?;
        self.compare_data_type(&l,&r,&left.position)?;
        Ok(())
    }
    fn analyze_expression(&self,expression:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                          symbol_table:&Rc<RefCell<SymbolTable>>)->Result<Type,Error> {
        return match expression
        {
            ExpressionNode::Literal(number) =>
                Ok(number.clone()),
            ExpressionNode::Unary(opr,right)=> {
                let right_type = self.analyze_expression(right,parent_function,symbol_table)?;
                match opr.kind {
                    TokenKind::BangToken => {
                        if right_type.get_type() != "bool" {
                            return Err(Error::new(ErrorKind::Other, format!("! operator requires bool, got {}", right_type.get_type())));
                        }
                        Ok(Type::Boolean(opr.clone()))
                    },
                    TokenKind::PlusToken | TokenKind::MinusToken => {
                        if right_type.get_type() != "int" && right_type.get_type() != "float" {
                            return Err(Error::new(ErrorKind::Other, format!("unary +/- requires int or float, got {}", right_type.get_type())));
                        }
                        Ok(right_type)
                    },
                    _ => Err(Error::new(ErrorKind::Other, format!("unknown unary operator {}", opr.text)))
                }
            },
            ExpressionNode::Binary(left,opr,right)=>
                Ok(self.analyze_binary_expression(left,opr,right,parent_function,symbol_table)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table)?),
            ExpressionNode::FunctionCall(name,params)=>
                Ok(self.analyze_function_call(name,params,parent_function,symbol_table)?),
            ExpressionNode::Parenthesized(expr)=>
                Ok(self.analyze_expression(expr,parent_function,symbol_table)?),
        };
    }
    fn analyze_binary_expression(&self,left:&ExpressionNode<'a>,opr:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
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
        
        match opr.kind {
            TokenKind::EqualEqualToken | TokenKind::NotEqualToken |
            TokenKind::GreaterThanToken | TokenKind::GreaterThanEqualToken |
            TokenKind::SmallerThanToken | TokenKind::SmallerThanEqualToken |
            TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken => {
                return Ok(Type::Boolean(opr.clone()));
            },
            _ => return Ok(left_value)
        }
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

    fn analyze_if_else(&self, condition:&ExpressionNode<'a>, if_body:&[StatementNode<'a>],
                       else_if:&Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>,
                       else_body: &Option<&'a [StatementNode<'a>]>,
                       parent_function:&FunctionNode<'a>, symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool) ->
    Result<(),Error>
    {
        //if condition
        let cond_type = self.analyze_expression(condition,parent_function,symbol_table)?;
        if cond_type.get_type() != "bool" {
            return Err(Error::new(ErrorKind::Other, format!("if condition must be bool, got {}", cond_type.get_type())));
        }
        //if body
        self.analyze_body(if_body,parent_function,Some(symbol_table),has_parent_while)?;

        //else if block
        for i in else_if.iter()
        {
            let elif_cond_type = self.analyze_expression(&i.0,parent_function,symbol_table)?;
            if elif_cond_type.get_type() != "bool" {
                return Err(Error::new(ErrorKind::Other, format!("else if condition must be bool, got {}", elif_cond_type.get_type())));
            }
            self.analyze_body(&i.1,parent_function,Some(symbol_table),has_parent_while)?;
        }
        match else_body
        {
            Some(body)=>self.analyze_body(body,parent_function,Some(symbol_table),has_parent_while)?,
            None=>()
        }
        Ok(())
    }
    fn analyze_return(&self,expression:&Option<ExpressionNode<'a>>,parent_function:&FunctionNode<'a>,
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