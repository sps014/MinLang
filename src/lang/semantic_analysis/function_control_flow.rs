use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, StatementNode};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct FlowNode
{
    child_nodes: Vec<Rc<RefCell<FlowNode>>>,
    has_return: Option<StatementNode>,
}
impl FlowNode
{
    fn new() -> Self
    {
        FlowNode
        {
            child_nodes: Vec::new(),
            has_return: None,
        }
    }
    fn from(has_return: Option<StatementNode>) -> Self
    {
        FlowNode
        {
            child_nodes: Vec::new(),
            has_return,
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
    pub fn create_new_node_parent(&mut self, parent_node:&Rc<RefCell<FlowNode>>, has_return:Option<StatementNode>) -> Rc<RefCell<FlowNode>>
    {
        let new_node = Rc::new(RefCell::new(FlowNode::from(has_return)));
        parent_node.borrow_mut().child_nodes.push(new_node.clone());
        new_node
    }
    pub fn build(&mut self)->Result<(),Error>
    {
        self.create_graph()?;
        dbg!(&self.root_node);
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
        self.root_node = Some(Rc::new(RefCell::new(FlowNode::from(None))));
        self.visit_nodes(&self.function.body.clone(),&self.root_node.clone().unwrap());
        Ok(())
    }
    fn visit_node(&mut self, statement:&StatementNode, parent:&Rc<RefCell<FlowNode>>) ->Result<Rc<RefCell<FlowNode>>,Error>
    {
        return  match statement {
            StatementNode::Return(_)=>
                self.visit_return(statement,parent),
            StatementNode::IfElse(_,if_body,else_pair,else_body)=>
                self.visit_if_else(if_body, else_pair, else_body, parent),
            StatementNode::Declaration(_,_)=>
                self.visit_declaration(parent),
            StatementNode::Assignment(_,_)=>
                self.visit_assignment(parent),
            _=>
                Err(Error::new(ErrorKind::Other,"not implemented")),
        };
    }
    fn visit_nodes(&mut self,nodes:&Vec<StatementNode>,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let mut node = parent.clone();
        for i in nodes.iter()
        {
            node=self.visit_node(i,&mut node)?;
        }
        Ok(node)
    }
    fn visit_if_else(&mut self, if_body:&Vec<StatementNode>,
                     else_if:&Vec<(ExpressionNode, Vec<StatementNode>)>,
                     else_body: &Option<Vec<StatementNode>>,parent:&Rc<RefCell<FlowNode>>)
        ->Result<Rc<RefCell<FlowNode>>,Error>
    {
        //if body
        let mut if_body_node = Rc::new(RefCell::new(FlowNode::new()));
        (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
        self.visit_nodes(if_body,&mut if_body_node)?;

        for i in else_if.iter()
        {
            if_body_node = Rc::new(RefCell::new(FlowNode::new()));
            (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
            self.visit_nodes(&i.1,&mut if_body_node)?;
        }
        if_body_node = match else_body {
            Some(else_body)=>
            {
                if_body_node = Rc::new(RefCell::new(FlowNode::new()));
                (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
                self.visit_nodes(else_body,&mut if_body_node)?
            },
            None=>if_body_node
        };

        Ok(if_body_node)
    }
    fn visit_return(&mut self,return_node:&StatementNode,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let mut return_flow = Rc::new(RefCell::new(FlowNode::new()));
        (*parent).as_ref().borrow_mut().child_nodes.push(return_flow.clone());
        self.visit_node(return_node,&mut return_flow)?;
        Ok(return_flow)
    }
    fn visit_declaration(&mut self,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let declaration_flow = Rc::new(RefCell::new(FlowNode::new()));
        (*parent).as_ref().borrow_mut().child_nodes.push(declaration_flow.clone());
        Ok(declaration_flow)
    }
    fn visit_assignment(&mut self,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let assignment_flow = Rc::new(RefCell::new(FlowNode::new()));
        (*parent).as_ref().borrow_mut().child_nodes.push(assignment_flow.clone());
        Ok(assignment_flow)
    }
}