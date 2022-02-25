use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

pub struct  SymbolTable
{
    symbols: HashMap<String,SyntaxToken>,
}

impl SymbolTable {
    pub fn new() -> SymbolTable {
        SymbolTable {
            symbols: HashMap::new(),
        }
    }
    fn get_key_name(name:String,parent_name:String) -> String {
        if parent_name.is_empty() {
            name
        } else {
            format!("{}.{}",parent_name,name)
        }
    }
    pub fn add_symbol(&mut self,name:String,token:SyntaxToken,parent:String)->Result<(),Error> {
        let key=SymbolTable::get_key_name(name.clone(),parent);

        return match self.symbols.insert(key,token)
        {
            Some(_) => Err(Error::new(ErrorKind::Other,format!("variable {} already exists",name))),
            None => Ok(()),
        }
    }
}