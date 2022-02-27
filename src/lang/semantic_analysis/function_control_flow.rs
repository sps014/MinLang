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
        let root_node = &(*self.root_node.as_ref().unwrap()).as_ref().clone();

        if self.dfs(&Rc::new(root_node.clone()))
        {
            return Ok(());
        }
        Err(Error::new(ErrorKind::Other,
                       format!("error : '{}': not all code paths return a value",self.function.name.text)))

    }
    //use dfs  and visit from right side depth by depth, if right most is true then all left will be true
    fn dfs(&mut self,node:&Rc<RefCell<FlowNode>>)->bool
    {
        let new=&node.as_ref().borrow();
        let mut ct_all_true=0;
        for i in (0..new.child_nodes.len()).rev()
        {
            let child=self.dfs(&new.child_nodes[i]);
            if child
            {
                ct_all_true+=1;
            }
            if child && new.child_nodes[i].as_ref().borrow().has_return
            {
                return true;
            }
        }
        //println!("{}: {}",new.name,ct_all_true);

        if new.child_nodes.len()!=0 && ct_all_true==new.child_nodes.len()
        {
            return true;
        }
        else if new.has_return
        {
            return true;
        }
        false
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
            _=>
                {},
        };
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