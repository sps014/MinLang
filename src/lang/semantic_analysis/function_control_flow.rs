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
    pub fn create_new_node_parent(&mut self, parent_node:&Rc<RefCell<FlowNode>>, has_return:bool) -> Rc<RefCell<FlowNode>>
    {
        let new_node = Rc::new(RefCell::new(FlowNode::from(has_return,"".to_string())));
        parent_node.borrow_mut().child_nodes.push(new_node.clone());
        new_node
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
        let mut j=0;
        for i in nodes.iter()
        {
            let new=self.visit_node(i,&mut node)?;
            if j!=-1 && new.is_some()
            {
                let nn= new.unwrap();
                parent.borrow_mut().child_nodes.push(nn.clone());
                node=nn.clone();
            }
            j+=1;
        }
        Ok(node)
    }
    fn visit_node(&mut self, statement:&StatementNode, parent:&Rc<RefCell<FlowNode>>) ->Result<Option<Rc<RefCell<FlowNode>>>,Error>
    {
        let r=  match statement {
            StatementNode::Return(r)=>
                {
                    Some(self.visit_return(&r.clone().unwrap(), parent)?);
                    None
                },
            StatementNode::IfElse(_,if_body,else_pair,else_body)=>
                {
                    Some(self.visit_if_else(if_body, else_pair, else_body, parent)?);
                    None
                },
            StatementNode::Declaration(_,_)=> {
                self.visit_declaration(parent)?;
                None
            },
            StatementNode::Assignment(_,_)=>
                {
                    self.visit_assignment(parent)?;
                    None
                },
            _=>
                return Err(Error::new(ErrorKind::Other,"not implemented")),
        };
        Ok(r)
    }
    fn visit_if_else(&mut self, if_body:&Vec<StatementNode>,
                     else_if:&Vec<(ExpressionNode, Vec<StatementNode>)>,
                     else_body: &Option<Vec<StatementNode>>,parent:&Rc<RefCell<FlowNode>>)
        ->Result<Rc<RefCell<FlowNode>>,Error>
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
        if_body_node = match else_body {
            Some(else_body)=>
            {
                if_body_node = Rc::new(RefCell::new(FlowNode::new("else".to_string())));
                (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
                self.visit_block(else_body, &mut if_body_node)?
            },
            None=>
                {
                    if_body_node = Rc::new(RefCell::new(FlowNode::new("else".to_string())));
                    (*parent).as_ref().borrow_mut().child_nodes.push(if_body_node.clone());
                    if_body_node
                }
        };

        Ok(if_body_node)
    }
    fn visit_return(&mut self,return_node:&ExpressionNode,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let mut return_flow = Rc::new(RefCell::new(FlowNode::from(true,format!("return {:?}",return_node))));
        (*parent).as_ref().borrow_mut().child_nodes.push(return_flow.clone());
        Ok(return_flow)
    }
    fn visit_declaration(&mut self,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let declaration_flow = Rc::new(RefCell::new(FlowNode::new("declare".to_string())));
        //(*parent).as_ref().borrow_mut().child_nodes.push(declaration_flow.clone());
        Ok(declaration_flow)
    }
    fn visit_assignment(&mut self,parent:&Rc<RefCell<FlowNode>>)->Result<Rc<RefCell<FlowNode>>,Error>
    {
        let assignment_flow = Rc::new(RefCell::new(FlowNode::new("assign".to_string())));
        //(*parent).as_ref().borrow_mut().child_nodes.push(assignment_flow.clone());
        Ok(assignment_flow)
    }
}