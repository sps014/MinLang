use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::{Rc};
use crate::lang::code_analysis::syntax::syntax_node::Type;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug)]
pub struct SymbolTable
{
    symbols: HashMap<String, Type>,
    parent: Option<Rc<RefCell<SymbolTable>>>,
    pub children: Vec<Rc<RefCell<SymbolTable>>>,
}

impl SymbolTable{
    pub fn new(parent:  Option<Rc<RefCell<SymbolTable>>>) -> SymbolTable {
        SymbolTable {
            symbols: HashMap::new(),
            parent,
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: Rc<RefCell<SymbolTable>>) {
        self.children.push(child);
    }
    pub fn get_all(&self)->Vec<(String,Type)>
    {
        let mut result = Vec::new();
        for (key, value) in self.symbols.iter() {
            result.push((key.clone(), value.clone()));
        }
        result
    }

    pub fn add_symbol(&mut self, name:String, token: Type) ->Result<(),Error> {

        return match self.symbols.insert(name.clone(),token)
        {
            Some(_) => Err(Error::new(ErrorKind::Other,format!("variable {} already exists at: {}",name
                                                               ,self.symbols.get(&name).unwrap().get_line_str()))),
            None => Ok(()),
        }
    }
    pub fn get_symbol(&self,name:SyntaxToken)->Result<Type,Error> {
        if self.symbols.contains_key(&name.text)
        {
            return Ok(self.symbols.get(&name.text).unwrap().clone());
        }

        match self.parent
        {
            Some(ref parent) =>
                {
                    let r=(*parent).as_ref().borrow().get_symbol(name)?;
                    return Ok(r);
                }
            None =>
                {
                    return Err(Error::new(ErrorKind::Other, format!("variable {} does not exist at: {}"
                                                                    , name.text,name.position.get_point_str())));
                }
        }

    }
}