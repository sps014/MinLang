use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::syntax::syntax_node::TypeLiteral;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

pub struct  SymbolTable
{
    symbols: HashMap<String,TypeLiteral>,
}

impl SymbolTable {
    pub fn new() -> SymbolTable {
        SymbolTable {
            symbols: HashMap::new(),
        }
    }

    pub fn add_symbol(&mut self,name:String,token:TypeLiteral)->Result<(),Error> {

        return match self.symbols.insert(name.clone(),token)
        {
            Some(_) => Err(Error::new(ErrorKind::Other,format!("variable {} already exists",name))),
            None => Ok(()),
        }
    }
    pub fn get_symbol(&self,name:String)->Result<TypeLiteral,Error> {
        return match self.symbols.get(&name)
        {
            Some(token) => Ok(token.clone()),
            None => Err(Error::new(ErrorKind::Other,format!("variable {} does not exist",name))),
        }
    }
}