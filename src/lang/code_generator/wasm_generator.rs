use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use crate::lang::code_analysis::syntax::syntax_node::{ExpressionNode, FunctionNode, ParameterNode, ProgramNode, StatementNode, Type};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::semantic_analysis::analyzer::SemanticInfo;
use crate::lang::semantic_analysis::function_table::FunctionTable;
use crate::lang::semantic_analysis::symbol_table::SymbolTable;

#[allow(dead_code)]
pub struct WasmGenerator<'a>
{
    syntax_tree:&'a SyntaxTree,
    symbol_map:&'a HashMap<String,Rc<RefCell<SymbolTable>>>,
    function_table:&'a FunctionTable,
    //key 1: function name, key 2: parameter name
    combined_symbol_lookup:HashMap<String,HashMap<String,Type>>,
    strings: HashMap<String, usize>,
    next_string_offset: usize,
}
impl<'a> WasmGenerator<'a>
{
    pub fn new (syntax_tree:&'a SyntaxTree,semantic_info:&'a SemanticInfo) -> Self
    {
        Self
        {
            syntax_tree,symbol_map:&semantic_info.hash_map, function_table:&semantic_info.function_table,
            combined_symbol_lookup:HashMap::new(),
            strings: HashMap::new(),
            next_string_offset: 0,
        }
    }
    pub fn build(&mut self)->Result<IndentedTextWriter,Error>
    {
        self.collect_strings_from_program(&self.syntax_tree.get_root());
        let mut indented=IndentedTextWriter::new();
        self.build_module(&self.syntax_tree.get_root(),&mut indented)?;
        Ok(indented)
    }
    fn collect_strings_from_program(&mut self, program: &ProgramNode) {
        for func in &program.functions {
            self.collect_strings_from_body(&func.body);
        }
    }
    fn collect_strings_from_body(&mut self, body: &Vec<StatementNode>) {
        for stmt in body {
            match stmt {
                StatementNode::Declaration(_, expr) | StatementNode::Assignment(_, expr) => {
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::IfElse(cond, if_body, else_ifs, else_body) => {
                    self.collect_strings_from_expr(cond);
                    self.collect_strings_from_body(if_body);
                    for (elif_cond, elif_body) in else_ifs {
                        self.collect_strings_from_expr(elif_cond);
                        self.collect_strings_from_body(elif_body);
                    }
                    if let Some(eb) = else_body {
                        self.collect_strings_from_body(eb);
                    }
                }
                StatementNode::While(cond, body) => {
                    self.collect_strings_from_expr(cond);
                    self.collect_strings_from_body(body);
                }
                StatementNode::For(init, cond, inc, body) => {
                    if let Some(init_stmt) = init {
                        self.collect_strings_from_body(&vec![*init_stmt.clone()]);
                    }
                    if let Some(cond_expr) = cond {
                        self.collect_strings_from_expr(cond_expr);
                    }
                    if let Some(inc_stmt) = inc {
                        self.collect_strings_from_body(&vec![*inc_stmt.clone()]);
                    }
                    self.collect_strings_from_body(body);
                }
                StatementNode::Return(Some(expr)) => {
                    self.collect_strings_from_expr(expr);
                }
                StatementNode::FunctionInvocation(_, params) => {
                    for param in params {
                        self.collect_strings_from_expr(param);
                    }
                }
                _ => {}
            }
        }
    }
    fn collect_strings_from_expr(&mut self, expr: &ExpressionNode) {
        match expr {
            ExpressionNode::Literal(Type::String(token)) => {
                let s = token.text.clone();
                if !self.strings.contains_key(&s) {
                    self.strings.insert(s.clone(), self.next_string_offset);
                    self.next_string_offset += s.len() + 1; // +1 for null terminator or length
                }
            }
            ExpressionNode::Binary(left, _, right) => {
                self.collect_strings_from_expr(left);
                self.collect_strings_from_expr(right);
            }
            ExpressionNode::Unary(_, right) => {
                self.collect_strings_from_expr(right);
            }
            ExpressionNode::Parenthesized(inner) => {
                self.collect_strings_from_expr(inner);
            }
            ExpressionNode::FunctionCall(_, params) => {
                for param in params {
                    self.collect_strings_from_expr(param);
                }
            }
            _ => {}
        }
    }
    fn build_module(&mut self,program:&ProgramNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write_line("(module");
        writer.indent();
        writer.write_line("(import \"env\" \"concat_strings\" (func $concat_strings (param i32 i32) (result i32)))");
        
        // Import stdlib functions
        for std_func in crate::lang::stdlib::StdlibFunction::get_all() {
            if std_func.name == "concat" { continue; } // handled by concat_strings
            
            let mut params_str = String::new();
            for p in &std_func.parameters {
                params_str.push_str(&format!("{} ", WasmGenerator::get_wasm_type_from(p.clone())?));
            }
            
            let result_str = match &std_func.return_type {
                Some(t) => format!(" (result {})", WasmGenerator::get_wasm_type_from(t.get_type())?),
                None => "".to_string()
            };
            
            writer.write_line(&format!("(import \"env\" \"{}\" (func ${} (param {}){}))", 
                std_func.name, std_func.name, params_str.trim(), result_str));
        }

        writer.write_line("(memory 1)");
        for (s, offset) in &self.strings {
            // Remove quotes from string literal for data segment, assuming it's "something"
            let unquoted = if s.starts_with('"') && s.ends_with('"') {
                &s[1..s.len()-1]
            } else {
                s.as_str()
            };
            writer.write_line(&format!("(data (i32.const {}) \"{}\\00\")", offset, unquoted));
        }
        
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
        match statement
        {
            StatementNode::Declaration(left,expression)=>
            self.build_declaration(left,function,expression,writer)?,
            StatementNode::Assignment(left,expression)=>
            self.build_assignment(left,expression,function,writer)?,
            StatementNode::Return(r)=>
            self.build_return(r,function,writer)?,
            StatementNode::While(c,b)=>
            self.build_while(c,b,function,writer)?,
            StatementNode::For(init,cond,inc,body)=>
            self.build_for(init,cond,inc,body,function,writer)?,
            StatementNode::Break=>
            self.build_break(writer)?,
            StatementNode::Continue=>
            self.build_continue(writer)?,
            StatementNode::IfElse(c,b,else_if,else_b)
            =>self.build_if_else(c,b,else_if,else_b,function,writer)?,
            StatementNode::FunctionInvocation(n,p)
            =>self.build_function_invocation(&n.text.clone(),p,function,writer)?,
        }
        Ok(())
    }
    fn build_function_invocation(&self,name:&String,parameters:&Vec<ExpressionNode>,
                                 function:&FunctionNode,writer:&mut IndentedTextWriter)
                                 ->Result<(),Error>
    {
        for i in parameters.iter()
        {
            self.build_expression(i,&"int".to_string(),function,writer)?;
        }
        writer.write("call $");
        writer.write_line(&name);
        Ok(())
    }
    fn build_if_else(&self,condition:&ExpressionNode,body:&Vec<StatementNode>,
                     else_if:&Vec<(ExpressionNode,Vec<StatementNode>)>,
                     else_body:&Option<Vec<StatementNode>>,
                     function:&FunctionNode,
                     writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        let mut arr=Vec::new();
        arr.push((Some(condition.clone()),body.clone()));
        for i in else_if.iter()
        {
            arr.push((Some(i.0.clone()),i.1.clone()));
        }
        if else_body.is_some()
        {
            arr.push((None,else_body.clone().unwrap()));
        }
        self.build_if_else_parts(&arr,function,0,writer)?;

        Ok(())
    }
    fn build_if_else_parts(&self,parts:&Vec<(Option<ExpressionNode>,Vec<StatementNode>)>,
                           function:&FunctionNode,index:usize,
                           writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        if index==parts.len()
        {
            return Ok(());
        }
        let cur=&parts[index];
        //generate else
        if cur.0.is_none() && index == parts.len() - 1
        {
            self.build_body(&cur.1,function,writer)?;
        }
        else
        {
            self.build_expression(&cur.0.clone().unwrap(),&"int".to_string(),function,writer)?;
            writer.write_line("(if");
            writer.indent();
            writer.write_line("(then");
            writer.indent();
            self.build_body(&cur.1,function,writer)?;
            writer.unindent();
            writer.write_line(")");
            if index+1<parts.len()
            {
                writer.write_line("(else");
                writer.indent();
                self.build_if_else_parts(parts,function,index+1,writer)?;
                writer.unindent();
                writer.write_line(")");
            }
            writer.unindent();
            writer.write_line(")");
        }
        Ok(())
    }
    fn build_break(&self,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write_line("br 1");
        Ok(())
    }
    fn build_continue(&self,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        writer.write_line("br 0");
        Ok(())
    }
    fn build_while(&self,condition:&ExpressionNode,body:&Vec<StatementNode>,
                   function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
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
    fn build_for(&self,init:&Option<Box<StatementNode>>,condition:&Option<ExpressionNode>,
                 increment:&Option<Box<StatementNode>>,body:&Vec<StatementNode>,
                 function:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        if let Some(init_stmt) = init {
            self.build_statement(init_stmt, function, writer)?;
        }
        writer.write_line("(block");
        writer.indent();
        writer.write_line("(loop");
        writer.indent();
        
        if let Some(cond_expr) = condition {
            self.build_expression(cond_expr, &"int".to_string(), function, writer)?;
            writer.write_line(format!("i32.const 0").as_str());
            writer.write_line(format!("i32.eq").as_str());
            writer.write_line("br_if 1");
        }
        
        self.build_body(body, function, writer)?;
        
        if let Some(inc_stmt) = increment {
            self.build_statement(inc_stmt, function, writer)?;
        }
        
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
        match expression
        {
            ExpressionNode::Identifier(identifier)=>
            self.build_identifier(identifier,writer)?,
            ExpressionNode::Unary(opr,expression)=>
            self.build_unary(opr,expression,left_side,function,writer)?,
            ExpressionNode::Binary(left,opr,right)=>
            self.build_binary(left,opr,right,left_side,function,writer)?,
            ExpressionNode::Literal(literal)=>
            self.build_literal(literal,writer)?,
            ExpressionNode::FunctionCall(n,args)=>
            self.build_function_invocation(&n.text.clone(),args,function,writer)?,
            ExpressionNode::Parenthesized(e)
            =>self.build_expression(e,left_side,function,writer)?,
        }
        Ok(())
    }
    fn build_literal(&self,literal:&Type,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        let type_=match literal {
          Type::Integer(i)=>format!("i32.const {}",i.text),
          Type::Float(f)=>format!("f32.const {}",f.text),
          Type::Boolean(f)=>format!("i32.const {}",if f.text=="true"{1}else{0}),
          Type::String(s)=> {
              let offset = self.strings.get(&s.text).unwrap();
              format!("i32.const {}", offset)
          },
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

        if left == "string" && opr.kind == TokenKind::PlusToken {
            writer.write_line("call $concat_strings");
            return Ok(());
        }

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
            TokenKind::EqualEqualToken =>
                writer.write_line(format!("{}.eq", symbol).as_str()),
            TokenKind::NotEqualToken =>
                writer.write_line(format!("{}.ne", symbol).as_str()),
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
                TokenKind::PlusToken | TokenKind::MinusToken | TokenKind::StarToken |
                TokenKind::EqualEqualToken | TokenKind::NotEqualToken => {},
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
                TokenKind::AmpersandAmpersandToken | TokenKind::BitWiseAmpersandToken =>
                    writer.write_line(format!("{}.and",symbol).as_str()),
                TokenKind::PipePipeToken | TokenKind::BitWisePipeToken =>
                    writer.write_line(format!("{}.or",symbol).as_str()),
                TokenKind::PlusToken | TokenKind::MinusToken | TokenKind::StarToken |
                TokenKind::EqualEqualToken | TokenKind::NotEqualToken => {},
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

        let mut param_names = std::collections::HashSet::new();
        for param in &function.parameters {
            param_names.insert(param.name.text.clone());
        }

        for (name,_type) in res.iter()
        {
            // Do not emit local variable declarations for function parameters
            if param_names.contains(name) {
                continue;
            }
            writer.write(" (local ");
            writer.write(format!("${} {}",
                                 name,
                                 WasmGenerator::get_wasm_type_from(_type.get_type())?)
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
            // Ignore parameter name redefinition errors across different functions
            // Wasm allows multiple local variables with the same name if they are in different scopes,
            // but our simple generator flattens them. We can just skip re-adding if it already exists
            // since we only care about allocating space for the type.
            if !res.contains_key(name) {
                res.insert(name.clone(),type_.clone());
            }
        }

        Ok(res)
    }
    fn build_export(&self,program:&ProgramNode,writer:&mut IndentedTextWriter)->Result<(),Error>
    {
        for i in program.functions.iter()
        {
            if i.is_exported {
                writer.write_line(format!("(export \"{}\" (func ${}))",
                                          i.name.text,
                                          i.name.text).as_str());
            }
        }
        Ok(())
    }
    fn get_wasm_type_from(typename:String)->Result<String,Error>
    {
        let r= match typename.as_str()
        {
            "int"=>"i32".to_string(),
            "float"=>"f32".to_string(),
            "bool"=>"i32".to_string(),
            "string"=>"i32".to_string(),
            _=>return Err(Error::new(ErrorKind::Other,format!("unsupported type {}",typename)))
        };
        Ok(r)
    }
}