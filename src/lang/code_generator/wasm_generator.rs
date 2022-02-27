use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, ParameterNode, ProgramNode, StatementNode, Type};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use crate::Parser;

pub struct WasmGenerator<'a>
{
    syntax_tree:&'a SyntaxTree,
    symbol_map:&'a HashMap<String,Rc<RefCell<SymbolTable>>>,
    //key 1: function name, key 2: parameter name
    combined_symbol_lookup:HashMap<String,HashMap<String,Type>>
}
impl<'a> WasmGenerator<'a>
{
    pub fn new (syntax_tree:&'a SyntaxTree,symbol_map:&'a HashMap<String,Rc<RefCell<SymbolTable>>>) -> Self
    {
        Self
        {
            syntax_tree,symbol_map,
            combined_symbol_lookup:HashMap::new()
        }
    }
    pub fn build(&mut self)->Result<IndentedTextWriter,Error>
    {
        let mut indented=IndentedTextWriter::new();
        self.build_module(&self.syntax_tree.clone().get_root(),&mut indented)?;
        Ok(indented)
    }
    fn build_module(&mut self,program:&ProgramNode,writer:&mut IndentedTextWriter)->Result<(),Error>
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
    fn build_function(&mut self,function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
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
        self.build_body(&function.body.clone(),function,writer)?;
        writer.unindent();

        writer.write_line(")");
        Ok(())
    }
    fn build_body(&self,statements:&Vec<StatementNode>,function:&FunctionNode,
                  writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        for i in statements.iter()
        {
            self.build_statement(i,function,writer)?;
        }
        Ok(())
    }

    fn build_statement(&self,statement:&StatementNode,
                       function:&FunctionNode,
                       writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        match statement.borrow()
        {
            StatementNode::Declaration(left,expression)=>
            self.build_declaration(left,function,expression,writer)?,
            StatementNode::Assignment(left,expression)=>
            self.build_assignment(left,expression,function,writer)?,
            StatementNode::Return(r)=>
            self.build_return(r,function,writer)?,
            StatementNode::While(c,b)=>
            self.build_while(c,b,function,writer)?,
            _=>return Err(Error::new(ErrorKind::Other,"unknown statement"))
        }
        Ok(())
    }
    fn build_while(&self,condition:&ExpressionNode,body:&Vec<StatementNode>,
                   function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        /*

        _writer.WriteLine("(block");
            _writer.Indent++;
            _writer.WriteLine("(loop");
            _writer.Indent++;
            Visit( new IExpressionNode
                .UnaryExpression(new SyntaxToken(TokenKind.BangToken,new TextSpan(),"!"),loop.Condition));
            _writer.WriteLine("br_if 1");
            Visit(loop.Body);
            _writer.WriteLine("br 0");
            _writer.Indent--;
            _writer.WriteLine(")");
            _writer.Indent--;
            _writer.WriteLine(")");
        */
        writer.write_line("(block");
        writer.indent();
        writer.write_line("(loop");
        writer.indent();
        self.build_expression(condition,&"int".to_string(),function,writer)?;
        writer.write_line(format!("i32.const 0").as_str());
        writer.write_line(format!("i32.eq").as_str());
        writer.write_line("br_if 1");
        self.build_body(body,function,writer)?;
        writer.write_line("br 0");
        writer.unindent();
        writer.write_line(")");
        writer.unindent();
        writer.write_line(")");
        Ok(())
    }
    fn build_return(&self,expression:&Option<ExpressionNode>,
                    function:&FunctionNode,
                    writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        if expression.is_some()
        {
            let return_type=&function.return_type.clone().unwrap();
            self.build_expression(&expression.clone().unwrap(),
                                  &return_type.get_type()
                                  ,function,writer)?;
        }
        writer.write_line("return");
        Ok(())
    }
    fn build_declaration(&self,left:&SyntaxToken,function:&FunctionNode,
                         expression:&ExpressionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {

        self.build_expression(&expression,&self.table_read_type(&left.text,function),function,writer)?;

        writer.write_line(format!("local.set ${}",left.text).as_str());
        Ok(())
    }
    fn build_assignment(&self,left:&SyntaxToken,expression:&ExpressionNode,
                        function:&FunctionNode,
                        writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        self.build_expression(&expression,&self.table_read_type(&left.text,function),function,writer)?;
        writer.write_line(format!("local.set ${}",left.text).as_str());
        Ok(())
    }
    fn build_expression(&self,expression:&ExpressionNode,
                        left_side:&String,function:&FunctionNode,
                        writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        match expression.borrow()
        {
            ExpressionNode::Identifier(identifier)=>
            self.build_identifier(identifier,writer)?,
            ExpressionNode::Unary(opr,expression)=>
            self.build_unary(opr,expression,left_side,function,writer)?,
            ExpressionNode::Binary(left,opr,right)=>
            self.build_binary(left,opr,right,left_side,function,writer)?,
            ExpressionNode::Literal(literal)=>
            self.build_literal(literal,writer)?,
            _=>return Err(Error::new(ErrorKind::Other,format!("unknown expression {:?}",expression)))
        }
        Ok(())
    }
    fn build_literal(&self,literal:&Type,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        let type_=match literal {
          Type::Integer(i)=>format!("i32.const {}",i.text),
          Type::Float(f)=>format!("f32.const {}",f.text),
            _=>return Err(Error::new(ErrorKind::Other,format!("unknown literal {:?}",literal)))
        };
        writer.write_line(type_.as_str());
        Ok(())
    }
    fn table_read_type(&self,var_name:&String,function:&FunctionNode)->String
    {
        let func_lookup=self.combined_symbol_lookup.get(&function.name.text).unwrap();
        return func_lookup.get(var_name).unwrap().clone().get_type();
    }
    fn build_binary(&self,left_exp:&ExpressionNode,opr:&SyntaxToken,right_expr:&ExpressionNode,
                   left:&String,function:&FunctionNode,
                   writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        self.build_expression(left_exp,left,function,writer)?;
        self.build_expression(right_expr,left,function,writer)?;

        let symbol=WasmGenerator::get_wasm_type_from(
            left.clone()
        )?;
        match opr.kind {
            TokenKind::PlusToken =>
                writer.write_line(format!("{}.add", symbol).as_str()),
            TokenKind::MinusToken =>
                writer.write_line(format!("{}.sub", symbol).as_str()),
            TokenKind::StarToken =>
                writer.write_line(format!("{}.mul", symbol).as_str()),
            _=>{}
        };
        if symbol=="f32"
        {
            match opr.kind {
                TokenKind::SlashToken=>
                    writer.write_line(format!("{}.div",symbol).as_str()),
                TokenKind::ModulusToken=>
                    writer.write_line(format!("{}.rem",symbol).as_str()),
                TokenKind::GreaterThanToken=>
                    writer.write_line(format!("{}.gt",symbol).as_str()),
                TokenKind::SmallerThanToken=>
                    writer.write_line(format!("{}.lt",symbol).as_str()),
                TokenKind::GreaterThanEqualToken=>
                    writer.write_line(format!("{}.ge",symbol).as_str()),
                TokenKind::SmallerThanEqualToken=>
                    writer.write_line(format!("{}.le",symbol).as_str()),
                TokenKind::PlusToken=>{},
                TokenKind::MinusToken=>{},
                TokenKind::StarToken=>{},
                _=>return Err(Error::new(ErrorKind::Other,format!("unknown operator {}",opr.text)))
            };
        }
        else if symbol=="i32"
        {
            match opr.kind {
                TokenKind::SlashToken=>
                    writer.write_line(format!("{}.div_s",symbol).as_str()),
                TokenKind::ModulusToken=>
                    writer.write_line(format!("{}.rem_s",symbol).as_str()),
                TokenKind::GreaterThanToken=>
                    writer.write_line(format!("{}.gt_s",symbol).as_str()),
                TokenKind::SmallerThanToken=>
                    writer.write_line(format!("{}.lt_s",symbol).as_str()),
                TokenKind::GreaterThanEqualToken=>
                    writer.write_line(format!("{}.ge_s",symbol).as_str()),
                TokenKind::SmallerThanEqualToken=>
                    writer.write_line(format!("{}.le_s",symbol).as_str()),
                TokenKind::PlusToken=>{},
                TokenKind::MinusToken=>{},
                TokenKind::StarToken=>{},
                _=>return Err(Error::new(ErrorKind::Other,format!("unknown operator {}",opr.text)))
            };
        }
        else
        {
            return Err(Error::new(ErrorKind::Other,format!("unknown symbol {}",symbol).as_str()));
        }

        Ok(())
    }
    fn build_unary(&self,opr:&SyntaxToken,expression:&ExpressionNode,
                   left:&String,function:&FunctionNode,
                   writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        self.build_expression(expression,left,function,writer)?;
        let symbol=WasmGenerator::get_wasm_type_from(
            left.clone()
        )?;
        match opr.kind {
            TokenKind::MinusToken=>
                {
                    writer.write_line(format!("{}.const -1",symbol).as_str());
                    writer.write_line(format!("{}.mul",symbol).as_str())
                },
            TokenKind::BangToken=>
                {
                    writer.write_line(format!("{}.const 0",symbol).as_str());
                    writer.write_line(format!("{}.eq",symbol).as_str());
                },
            TokenKind::PlusToken=>{},
            _=>return Err(Error::new(ErrorKind::Other,
                                     format!("wasm does nor support unary operator {}",opr.text).as_str()))
        };
        Ok(())
    }
    fn build_identifier(&self,identifier:&SyntaxToken,
                        writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write_line(format!("local.get ${}",identifier.text).as_str());
        Ok(())
    }
    fn build_return_type(&self, function:&FunctionNode,
                         writer:&mut IndentedTextWriter) ->Result<(),Error>
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
    fn build_local_variable(&mut self,function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
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
        self.combined_symbol_lookup.insert(function.name.text.clone(),res);
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