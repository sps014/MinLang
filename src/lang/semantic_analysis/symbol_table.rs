use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::{Rc, Weak};
use crate::lang::code_analysis::syntax::syntax_node::TypeLiteral;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug)]
pub struct SymbolTable
{
    symbols: HashMap<String,TypeLiteral>,
    parent: Option<Rc<RefCell<SymbolTable>>>,
}

impl SymbolTable{
    pub fn new(parent:  Option<Rc<RefCell<SymbolTable>>>) -> SymbolTable {
        SymbolTable {
            symbols: HashMap::new(),
            parent
        }
    }

    pub fn add_symbol(&mut self,name:String,token:TypeLiteral)->Result<(),Error> {

        return match self.symbols.insert(name.clone(),token)
        {
            Some(_) => Err(Error::new(ErrorKind::Other,format!("variable {} already exists at: {}",name
                                                               ,self.symbols.get(&name).unwrap().get_line_str()))),
            None => Ok(()),
        }
    }
    pub fn get_symbol(&self,name:String)->Result<TypeLiteral,Error> {
        if self.symbols.contains_key(&name)
        {
            return Ok(self.symbols.get(&name).unwrap().clone());
        }
        if self.parent.is_none()
        {
            return  Err(Error::new(ErrorKind::Other,format!("variable {} does not exist",name)));
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
                    return Err(Error::new(ErrorKind::Other, format!("variable {} does not exist", name)));
                }
        }

    }
}