use std::borrow::Borrow;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::TypeLiteral;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug, Clone)]
pub struct  SymbolTable
{
    symbols: HashMap<String,TypeLiteral>,
    parent: Option<Rc<SymbolTable>>,
}

impl SymbolTable {
    pub fn new(parent:Option<Rc<SymbolTable>>) -> SymbolTable {
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

        let p=self.parent.borrow();
        if p.is_none()
        {
            return  Err(Error::new(ErrorKind::Other,format!("variable {} does not exist",name)));
        }
        return Ok(p.as_ref().unwrap().get_symbol(name)?);
    }
}