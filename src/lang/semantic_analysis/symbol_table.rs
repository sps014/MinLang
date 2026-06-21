use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{Error, ErrorKind};
use std::rc::{Rc};
use crate::lang::code_analysis::syntax::nodes::Type;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

#[derive(Debug)]
pub struct SymbolTable
{
    symbols: HashMap<String, Type>,
    /// Names declared with `const` in this scope; reassigning them is an error.
    const_symbols: HashSet<String>,
    parent: Option<Rc<RefCell<SymbolTable>>>,
    pub children: Vec<Rc<RefCell<SymbolTable>>>,
}

impl SymbolTable{
    pub fn new(parent:  Option<Rc<RefCell<SymbolTable>>>) -> SymbolTable {
        SymbolTable {
            symbols: HashMap::new(),
            const_symbols: HashSet::new(),
            parent,
            children: Vec::new(),
        }
    }

    /// Marks a name as immutable (`const`) within this scope.
    pub fn mark_const(&mut self, name: String) {
        self.const_symbols.insert(name);
    }

    /// Returns true if `name` resolves to a `const` binding in this scope or an enclosing one.
    pub fn is_const(&self, name: &str) -> bool {
        if self.const_symbols.contains(name) {
            return true;
        }
        // Only consult the parent if the name is not shadowed by a local declaration here.
        if self.symbols.contains_key(name) {
            return false;
        }
        match self.parent {
            Some(ref parent) => parent.as_ref().borrow().is_const(name),
            None => false,
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
    pub fn get_symbol(&self, name: &SyntaxToken) -> Result<Type, Error> {
        if let Some(symbol) = self.symbols.get(&name.text) {
            return Ok(symbol.clone());
        }

        match self.parent {
            Some(ref parent) => parent.as_ref().borrow().get_symbol(name),
            None => Err(Error::new(
                ErrorKind::Other,
                format!("variable {} does not exist at: {}", name.text, name.position.get_point_str()),
            )),
        }
    }
}