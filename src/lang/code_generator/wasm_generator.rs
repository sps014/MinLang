use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, ParameterNode, ProgramNode, StatementNode, Type};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
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
        self.build_return_type(function, writer)?;
        self.build_local_variable(function,writer)?;
        writer.write_line("");

        writer.indent();
        self.build_body(&function.body.clone(),writer)?;
        writer.unindent();

        writer.write_line(")");
        Ok(())
    }
    fn build_body(&self,statements:&Vec<StatementNode>,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        for i in statements.iter()
        {
            self.build_statement(i,writer)?;
        }
        Ok(())
    }

    fn build_statement(&self,statement:&StatementNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        match statement.borrow()
        {
            StatementNode::Declaration(left,expression)=>
            self.build_declaration(left,expression,writer)?,
            StatementNode::Assignment(left,expression)=>
            self.build_assignment(left,expression,writer)?,
            _=>return Err(Error::new(ErrorKind::Other,"unknown statement"))
        }
        Ok(())
    }
    fn build_declaration(&self,left:&SyntaxToken,expression:&ExpressionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        self.build_expression(&expression,writer)?;
        writer.write_line(format!("local.set ${}",left.text).as_str());
        Ok(())
    }
    fn build_assignment(&self,left:&SyntaxToken,expression:&ExpressionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        self.build_expression(&expression,writer)?;
        writer.write_line(format!("local.set ${}",left.text).as_str());
        Ok(())
    }
    fn build_expression(&self,expression:&ExpressionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        match expression.borrow()
        {
            ExpressionNode::Identifier(identifier)=>
            self.build_identifier(identifier,writer)?,
            _=>return Err(Error::new(ErrorKind::Other,"unknown expression"))
        }
        Ok(())
    }
    fn build_identifier(&self,identifier:&SyntaxToken,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write_line(format!("local.get ${}",identifier.text).as_str());
        Ok(())
    }
    fn build_return_type(&self, function:&FunctionNode, writer:&mut IndentedTextWriter) ->Result<(),Error>
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

        let res=self.get_local_variables(self.symbol_map.get(&function.name.text.clone()).unwrap())?;
        for (name,_type) in res.iter()
        {
            writer.write(" (local ");
            writer.write(format!("${} {}",
                                 name,
                                 WasmGenerator::get_wasm_type_from(_type.borrow().get_type())?)
                .as_str());
            writer.write(") ");
        }
        Ok(())
    }
    fn get_local_variables(&self,symbol:&Rc<RefCell<SymbolTable>>)->Result<HashMap<String,Type>,Error>
    {
        let mut res=HashMap::new();
        let current_scope=(*symbol).as_ref().borrow();
        let mut local_variables=current_scope.get_all();

        for children in current_scope.children.iter()
        {
            let child_local_variables=self.get_local_variables(children)?;
            local_variables.extend(child_local_variables);
        }
        for (name,type_) in local_variables.iter()
        {
            if res.contains_key(name)
            {
                return Err(Error::new(ErrorKind::Other,
                                      format!("wasm does not support local variable of same name, even in different scope, '{}' defined more than 1 time",
                                      name)));
            }
            res.insert(name.clone(),type_.clone());
        }

        Ok(res)
    }
    fn build_export(&self,program:&ProgramNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        //            _writer.WriteLine($@"(export ""{function.Name}"" (func ${function.Name}))");
        for i in program.functions.iter()
        {
            writer.write_line(format!("(export \"{}\" (func ${}))",
                                      i.name.text,
                                      i.name.text).as_str());
        }
        Ok(())
    }
    fn get_wasm_type_from(typename:String)->Result<String,Error>
    {
        let r= match typename.as_str()
        {
            "int"=>"i32".to_string(),
            "float"=>"f32".to_string(),
            _=>return Err(Error::new(ErrorKind::Other,format!("unsupported type {}",typename)))
        };
        Ok(r)
    }
}