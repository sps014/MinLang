use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::syntax::nodes::{FunctionNode, Type};
use crate::lang::stdlib::StdlibFunction;

#[derive(Debug, Clone)]
pub struct FunctionTable {
    pub functions: HashMap<String, FunctionTableInfo>,
}


impl FunctionTable {
    pub fn new() -> FunctionTable {
        let mut table = FunctionTable {
            functions: HashMap::new(),
        };
        
        for std_func in StdlibFunction::get_all() {
            let info = FunctionTableInfo::new(
                std_func.name.clone(),
                std_func.return_type,
                std_func.parameters,
            );
            table.functions.insert(std_func.name, info);
        }
        
        table
    }

    pub fn add_function(&mut self, name: String, function_info: FunctionTableInfo) -> Result<(), Error>
    {
        if self.functions.contains_key(&name)
        {
            return Err(Error::new(ErrorKind::Other, format!("Function already exists ({})", name)));
        }
        self.functions.insert(name, function_info);
        Ok(())
    }


    pub fn get_function(&self, name: &String) -> Result<FunctionTableInfo, Error> {
        if !self.functions.contains_key(name)
        {
            return Err(Error::new(ErrorKind::Other, format!("Function does not exist ({})", name)));
        }
        Ok(self.functions.get(name).unwrap().clone())
    }
}


#[derive(Debug,Clone)]
pub  struct FunctionTableInfo
{
    #[allow(dead_code)]
    pub name: String,
    pub return_type: Option<Type>,
    pub parameters: Vec<String>,
}

impl FunctionTableInfo {
    pub fn new(name: String, return_type: Option<Type>, parameters: Vec<String>) -> FunctionTableInfo {
        FunctionTableInfo {
            name,
            return_type,
            parameters,
        }
    }
    pub fn from(func:&FunctionNode)->Self
    {
        let name = func.name.clone();
        let return_type = func.return_type.clone();
        let mut parameters:Vec<String> = vec![];
        for i in func.parameters.iter()
        {
            let j=i.clone();
            parameters.push(j.type_.text);
        }
        FunctionTableInfo::new(name.text, return_type, parameters)
    }
}
