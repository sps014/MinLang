use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{FunctionNode, ParameterNode, ProgramNode, StatementNode, Type};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use crate::Parser;

pub struct WasmGenerator<'a>
{
    syntax_tree:&'a SyntaxTree,
    symbol_map:&'a HashMap<String,Rc<RefCell<SymbolTable>>>
}
impl<'a> WasmGenerator<'a>
{
    pub fn new (syntax_tree:&'a SyntaxTree,symbol_map:&'a HashMap<String,Rc<RefCell<SymbolTable>>>) -> Self
    {
        Self
        {
            syntax_tree,symbol_map
        }
    }
    pub fn build(&self)->Result<IndentedTextWriter,Error>
    {
        let mut indented=IndentedTextWriter::new();
        self.build_module(&self.syntax_tree.clone().get_root(),&mut indented)?;
        Ok(indented)
    }
    fn build_module(&self,program:&ProgramNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write_line("(module");
        writer.indent();
        for i in program.functions.iter()
        {
            self.build_function(i,writer)?;
        }
        self.build_export(program,writer)?;
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }
    fn build_function(&self,function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write("(func $");
        writer.write(&function.name.text);
        for i in function.parameters.iter()
        {
            self.build_parameter(i,writer)?;
        }
        self.build_return(function,writer)?;
        self.build_local_variable(function,writer)?;

        Ok(())
    }
    fn build_return(&self,function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        if function.return_type.is_some()
        {
            let return_type=function.return_type.as_ref().unwrap();
            let return_type_name=WasmGenerator::get_wasm_type_from(return_type.get_type())?;
            writer.write(" (result ");
            writer.write(return_type_name.as_str());
            writer.write(")");
        }
        Ok(())
    }
    fn build_parameter(&self,parameter:&ParameterNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write("( ");
        writer.write(format!("param ${} {}",
                             parameter.name.text,
                             WasmGenerator::get_wasm_type_from(parameter.type_.text.clone())?)
            .as_str());
        writer.write(") ");
        Ok(())
    }
    fn build_local_variable(&self,function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {

        // writer.write("(local ");
        // writer.write(format!("${} {}",
        //                      statement.name.text,
        //                      WasmGenerator::get_wasm_type_from(statement.type_.text.clone())?)
        //     .as_str());
        // writer.write(") ");
        let res=self.get_local_variables(self.symbol_map.get(&function.name.text.clone()).unwrap());
        dbg!(&res);
        Ok(())
    }
    fn get_local_variables(&self,symbol:&Rc<RefCell<SymbolTable>>)->Vec<(String,Type)>
    {
        let current_scope=(*symbol).as_ref().borrow();
        let mut local_variables=current_scope.get_all();

        for children in current_scope.children.iter()
        {
            let child_local_variables=self.get_local_variables(children);
            local_variables.extend(child_local_variables);
        }
        local_variables
    }
    fn build_export(&self,function:&ProgramNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        Ok(())
    }
    fn get_wasm_type_from(typename:String)->Result<String,Error>
    {
        let r= match typename.as_str()
        {
            "int"=>"i32".to_string(),
            "float"=>"f32".to_string(),
            _=>return Err(Error::new(ErrorKind::Other,"unknown type"))
        };
        Ok(r)
    }
}