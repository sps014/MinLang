// use std::cell::RefCell;
// use std::collections::HashMap;
// use std::fs::write;
// use std::io::Error;
// use std::rc::Rc;
// use crate::lang::code_analysis::syntax::syntax_node::{FunctionNode, ParameterNode, ProgramNode, StatementNode, Type};
// use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
// use crate::lang::code_analysis::text::indented_text_writer::IndentedTextWriter;
// use crate::lang::semantic_analysis::analyzer::SemanticInfo;
// use crate::lang::semantic_analysis::function_table::FunctionTable;
// use crate::lang::semantic_analysis::symbol_table::SymbolTable;
//
// pub struct CppGenerator<'a>
// {
//     syntax_tree:&'a SyntaxTree,
//     symbol_map:&'a HashMap<String,Rc<RefCell<SymbolTable>>>,
//     function_table:&'a FunctionTable,
//     //key 1: function name, key 2: parameter name
//     combined_symbol_lookup:HashMap<String,HashMap<String,Type>>
// }
// impl<'a> CppGenerator<'a>
// {
//     pub fn new(syntax_tree: &'a SyntaxTree, semantic_info: &'a SemanticInfo) -> Self
//     {
//         Self
//         {
//             syntax_tree,
//             symbol_map: &semantic_info.hash_map,
//             function_table: &semantic_info.function_table,
//             combined_symbol_lookup: HashMap::new()
//         }
//     }
//     pub fn build(&mut self) -> Result<IndentedTextWriter, Error>
//     {
//         let mut indented = IndentedTextWriter::new();
//         self.build_module(&self.syntax_tree.clone().get_root(), &mut indented)?;
//         Ok(indented)
//     }
//      fn build_module(&mut self, syntax_node: &ProgramNode, indented: &mut IndentedTextWriter) -> Result<(), Error>
//     {
//         let functions = syntax_node.functions.clone();
//         for i in functions.iter()
//         {
//             self.build_function(i, indented)?;
//         }
//         Ok(())
//     }
//      fn build_function(&mut self,func:&FunctionNode,writer:&mut IndentedTextWriter)->Result<(),Error>
//     {
//         let return_type = func.return_type.clone();
//         let mut params = func.parameters.clone();
//         let name = func.name.clone();
//         match return_type {
//             None => {
//                 writer.write(&format!("void {}(",name.text));
//             },
//             Some(t) => {
//                 writer.write(&format!("{} {}(",t.get_type(),name.text));
//             }
//
//         };
//         self.build_params(&mut params,writer)?;
//         writer.write(")");
//         Ok(())
//     }
//     fn build_params(&self,params:&Vec<ParameterNode>,writer:&mut IndentedTextWriter)->Result<(),Error>
//     {
//         //comma separated write
//         for i in 0..params.len()
//         {
//             if i==params.len()-1
//             {
//                 writer.write(&format!("{} {}",params[i].type_.text,params[i].name.text));
//             }
//         }
//
//         Ok(())
//     }
//     fn build_block(&self,block:&Vec<StatementNode>,writer:&mut IndentedTextWriter)->Result<(),Error>
//     {
//         for i in block.iter()
//         {
//             self.build_statement(i,writer)?;
//         }
//         Ok(())
//     }
//     fn build_statement(&self,statement:&StatementNode,writer:&mut IndentedTextWriter)->Result<(),Error>
//     {
//         match statement
//         {
//             StatementNode::Declaration(v,ex) => {
//                 self.build_variable_declaration(v,writer)?;
//             },
//             StatementNode::Assignment(e,ass) => {
//                 self.build_expression(e,writer)?;
//             },
//             StatementNode::Return(r) => {
//                 self.build_return(r,writer)?;
//             },
//             StatementNode::IfElse(i_if_cond,if_b,elseif,_else) => {
//                 self.build_if(i,writer)?;
//             },
//             StatementNode::Break => {
//                 self.build_while(w,writer)?;
//             },
//             StatementNode::Continue => {
//                 self.build_for(f,writer)?;
//             },
//             StatementNode::FunctionInvocation(b,p) => {
//                 self.build_block(b,writer)?;
//             },
//             StatementNode::While(w,b) => {
//                 self.build_while(w,writer)?;
//             },
//         }
//         Ok(())
//     }
//
//
// }