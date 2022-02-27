use std::borrow::Borrow;
use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, StatementNode};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct FlowNode
{
    child_nodes: Vec<Rc<RefCell<FlowNode>>>,
    name: String,
    has_return: bool,
}
impl FlowNode
{
    fn new(name:String) -> Self
    {
        FlowNode
        {
            child_nodes: Vec::new(),
            has_return: false,
            name,
        }
    }
    fn from(has_return: bool,name:String) -> Self
    {
        FlowNode
        {
            child_nodes: Vec::new(),
            has_return,
            name,
        }
    }
    pub fn print(&self,indent: usize)
    {
        for _ in 0..indent
        {
            print!(" ");
        }
        println!("|____{}: {}", self.name,self.has_return);
        for child in &self.child_nodes
        {
            child.as_ref().borrow().print(indent + 8);
        }
    }
}
#[derive(Debug)]
pub struct  FunctionControlGraph
{
    root_node: Option<Rc<RefCell<FlowNode>>>,
    function:FunctionNode,
}

impl FunctionControlGraph
{
    pub fn new(function:&FunctionNode) -> FunctionControlGraph
    {
        Self {
            root_node: None,
            function: function.clone(),
        }
    }
    pub fn build(&mut self)->Result<(),Error>
    {
        self.create_graph()?;
        (*self.root_node.as_ref().unwrap()).as_ref().borrow().print(0);
        match self.function.return_type {
            Some(_)=>self.check_non_void_return()?,
            None=> {  },
        };
        Ok(())
    }
    fn check_non_void_return(&mut self)->Result<(),Error>
    {

        Ok(())
    }

    fn create_graph(&mut self)->Result<(),Error>
    {
        self.root_node = Some(Rc::new(RefCell::new(FlowNode::from(false,"root".to_string()))));
        self.visit_block(&self.function.body.clone(), &self.root_node.clone().unwrap())?;
        Ok(())
    }
    fn visit_block(&mut self, nodes:&Vec<StatementNode>, parent:&Rc<RefCell<FlowNode>>) ->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let mut node = parent.clone();
        for i in nodes.iter()
        {
            self.visit_node(i,&mut node)?;
        }
        Ok(node)
    }
    fn visit_node(&mut self, statement:&StatementNode, parent:&Rc<RefCell<FlowNode>>) ->Result<(),Error>
    {
          match statement {
            StatementNode::Return(r)=>
                self.visit_return(&r.clone().unwrap(), parent)?,
            StatementNode::IfElse(_,if_body,else_pair,else_body)=>
                self.visit_if_else(if_body, else_pair, else_body, parent)?,
              StatementNode::While(cond,expr)=>
                self.visit_while(cond,expr,parent)?,
            _=>
                {},
        };
        Ok(())
    }
    fn visit_while(&mut self, cond:&ExpressionNode, expr:&Vec<StatementNode>, parent:&Rc<RefCell<FlowNode>>) ->Result<(),Error>
    {
        let mut node = Rc::new(RefCell::new(FlowNode::new("while".to_string())));
        (*parent).as_ref().borrow_mut().child_nodes.push(node.clone());
        self.visit_block(expr,&mut node)?;

        //if while does not match
        node = Rc::new(RefCell::new(FlowNode::new("while negation".to_string())));
        (*parent).as_ref().borrow_mut().child_nodes.push(node.clone());
        Ok(())
    }
    fn visit_if_else(&mut self, if_body:&Vec<StatementNode>,
                     else_if:&Vec<(ExpressionNode, Vec<StatementNode>)>,
                     else_body: &Option<Vec<StatementNode>>,parent:&Rc<RefCell<FlowNode>>)
        ->Result<(),Error>
    {
        //if body
        let mut if_body_node = Rc::new(RefCell::new(FlowNode::new("if".to_string())));
        (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
        self.visit_block(if_body, &mut if_body_node)?;

        for i in else_if.iter()
        {
            if_body_node = Rc::new(RefCell::new(FlowNode::new("else if".to_string())));
            (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
            self.visit_block(&i.1, &mut if_body_node)?;
        }
        match else_body {
            Some(else_body)=>
            {
                if_body_node = Rc::new(RefCell::new(FlowNode::new("else".to_string())));
                (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
                self.visit_block(else_body, &mut if_body_node)?;
            },
            None=>
                {
                    if_body_node = Rc::new(RefCell::new(FlowNode::new("else".to_string())));
                    (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
                }
        };

        Ok(())
    }
    fn visit_return(&mut self,return_node:&ExpressionNode,parent:&Rc<RefCell<FlowNode>>)->Result<(),Error>
    {
        let mut return_flow = Rc::new(RefCell::new(FlowNode::from(true,format!("return {:?}",return_node))));
        (*parent).as_ref().borrow_mut().child_nodes.push(return_flow.clone());
        Ok(())
    }

}