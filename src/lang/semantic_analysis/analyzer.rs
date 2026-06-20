use bumpalo::Bump;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::lang::code_analysis::syntax::nodes::{ExpressionNode, FunctionNode, Type, ProgramNode, StatementNode};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::semantic_analysis::function_control_flow::FunctionControlGraph;
use crate::lang::semantic_analysis::function_table::{FunctionTable, FunctionTableInfo};
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use crate::lang::semantic_analysis::struct_table::StructTable;
use crate::lang::diagnostics::DiagnosticBag;

pub struct SemanticInfo<'a>
{
    pub hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
    pub struct_table: &'a StructTable,
    pub instantiated_generics: HashMap<String, (String, &'a FunctionNode<'a>)>,
    pub struct_methods: Vec<&'a FunctionNode<'a>>,
}

impl<'a> SemanticInfo<'a> {
    pub fn new(hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>, function_table: &'a FunctionTable, struct_table: &'a StructTable, instantiated_generics: HashMap<String, (String, &'a FunctionNode<'a>)>, struct_methods: Vec<&'a FunctionNode<'a>>) -> SemanticInfo<'a>
    {
        SemanticInfo {
            hash_map,
            function_table,
            struct_table,
            instantiated_generics,
            struct_methods,
        }
    }
}


pub struct Anaylzer<'a> {
    syntax_tree:&'a SyntaxTree<'a>,
    function_table:FunctionTable,
    struct_table:StructTable,
    arena: &'a Bump,
    generic_functions: HashMap<String, &'a FunctionNode<'a>>,
    instantiated_generics: HashMap<String, (String, &'a FunctionNode<'a>)>,
    generic_structs: HashMap<String, &'a crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    struct_methods: Vec<&'a FunctionNode<'a>>,
}
impl<'a> Anaylzer<'a> {
    pub fn new(tree: &'a SyntaxTree<'a>, arena: &'a Bump) -> Self {
        Self { syntax_tree:tree, function_table: FunctionTable::new(), struct_table: StructTable::new(), arena, generic_functions: HashMap::new(), instantiated_generics: HashMap::new(), generic_structs: HashMap::new(), struct_methods: Vec::new() }
    }
    pub fn analyze(&mut self, diagnostics: &mut DiagnosticBag) -> Result<SemanticInfo<'_>, ()> {
        let pgm= self.syntax_tree.get_root();
        self.analyze_pgm(pgm, diagnostics)
    }
    fn analyze_pgm(&mut self,node:&'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) -> Result<SemanticInfo<'_>, ()> {
        let mut symbol_table_map=HashMap::new();
        
        // Pass 0: Register all structs
        for struct_decl in node.structs.iter() {
            if struct_decl.generic_parameters.is_some() {
                self.generic_structs.insert(struct_decl.name.text.clone(), struct_decl);
                continue;
            }
            if let Err(e) = self.struct_table.add_struct(struct_decl) {
                diagnostics.report_error(e, Some(struct_decl.name.position.clone()));
            }
            self.register_struct_methods(struct_decl, &struct_decl.name.text, None, diagnostics);
        }
        
        // Pass 1: Register all functions in the function table
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                self.generic_functions.insert(function.name.text.clone(), function);
                continue;
            }

            if function.is_exported {
                // Check if any parameter or return type is a struct, and if so, check if it's exported
                let mut types_to_check = vec![];
                if let Some(ret_type) = &function.return_type {
                    types_to_check.push(ret_type);
                }
                for param in &function.parameters {
                    types_to_check.push(&param.type_);
                }
                
                for type_to_check in types_to_check {
                    let base_type_str = if type_to_check.get_type().ends_with("?") {
                        type_to_check.get_type()[..type_to_check.get_type().len() - 1].to_string()
                    } else if type_to_check.get_type().ends_with("[]") {
                        type_to_check.get_type()[..type_to_check.get_type().len() - 2].to_string()
                    } else {
                        type_to_check.get_type()
                    };
                    
                    if let Some(struct_info) = self.struct_table.get_struct(&base_type_str) {
                        if !struct_info.is_exported {
                            diagnostics.report_error(format!("Exported function '{}' exposes unexported struct '{}'", function.name.text, base_type_str), Some(function.name.position.clone()));
                        }
                    }
                }
            }
            
            if let Err(e) = self.function_table.add_function(function.name.text.clone(),FunctionTableInfo::from(function)) {
                diagnostics.report_error(e.to_string(), Some(function.name.position.clone()));
            }
        }
        
        // Pass 2: Analyze function bodies
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                continue;
            }
            let r=self.analyze_function(function, diagnostics)?;
            symbol_table_map.insert(function.name.text.clone(),r);
        }
        
        // Pass 3: Analyze generic instances
        // The AST nodes for generics will be analyzed here, so any errors inside generics 
        // with specific concrete types will be reported.
        let generics_to_analyze: Vec<_> = self.instantiated_generics.iter()
            .map(|(k, v)| (k.clone(), v.0.clone(), v.1))
            .collect();
            
        for (mangled_name, concrete_type, template) in generics_to_analyze {
            let r = self.analyze_function(template, diagnostics)?;
            symbol_table_map.insert(mangled_name.clone(), r);
        }

        // Pass 4: Analyze struct methods
        // Copy the struct_methods to avoid borrowing issues during analysis
        let methods_to_analyze = self.struct_methods.clone();
        for method in methods_to_analyze {
            let r = self.analyze_function(method, diagnostics)?;
            symbol_table_map.insert(method.name.text.clone(), r);
        }

        Ok(SemanticInfo::new(symbol_table_map,&self.function_table, &self.struct_table, self.instantiated_generics.clone(), self.struct_methods.clone()))
    }
    fn ensure_struct_instantiated(&mut self, mangled_name: &str, position: &TextSpan, diagnostics: &mut DiagnosticBag) {
        if self.struct_table.get_struct(&mangled_name.to_string()).is_some() {
            return;
        }

        // Try to derive base name and argument from mangled name
        let parts: Vec<&str> = mangled_name.split('_').collect();
        if parts.len() < 2 {
            return;
        }
        
        let base_name = parts[0];
        let concrete_type_str = parts[1..].join("_");

        if let Some(template) = self.generic_structs.get(base_name) {
            let mut new_fields = Vec::new();
            for field in &template.fields {
                let mut field_type_token = field.type_token.clone();
                
                // If it's the generic parameter T (or whatever it's called)
                let gen_param_name = template.generic_parameters.as_ref().unwrap()[0].text.clone();
                
                let is_generic = field_type_token.text == gen_param_name 
                    || field_type_token.text == format!("{}[]", gen_param_name) 
                    || field_type_token.text == format!("{}?", gen_param_name);
                
                if is_generic {
                    if field_type_token.text == gen_param_name {
                        field_type_token.text = concrete_type_str.clone();
                    } else if field_type_token.text.ends_with("[]") {
                        field_type_token.text = format!("{}[]", concrete_type_str);
                    } else if field_type_token.text.ends_with("?") {
                        field_type_token.text = format!("{}?", concrete_type_str);
                    }
                }
                
                new_fields.push(crate::lang::code_analysis::syntax::nodes::struct_node::StructFieldNode {
                    name: field.name.clone(),
                    type_token: field_type_token,
                });
            }

            let mut new_name_token = template.name.clone();
            new_name_token.text = mangled_name.to_string();
            let new_decl = crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode::new(
                new_name_token,
                None, // Stripped of generics
                new_fields,
                template.methods.clone(), // Methods will be processed separately
                template.is_exported
            );

            if let Err(e) = self.struct_table.add_struct(&new_decl) {
                diagnostics.report_error(e, Some(position.clone()));
            }
            
            self.register_struct_methods(&new_decl, mangled_name, Some(&concrete_type_str), diagnostics);
        }
    }

    fn register_struct_methods(&mut self, struct_decl: &crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>, struct_type_str: &str, concrete_type_str: Option<&str>, diagnostics: &mut DiagnosticBag) {
        for method in &struct_decl.methods {
            let mangled_name = format!("{}_{}", struct_type_str, method.name.text);
            
            // Create a new FunctionNode with the mangled name and injected `this` parameter
            let mut new_method = method.clone();
            let mut new_name_token = method.name.clone();
            new_name_token.text = mangled_name.clone();
            new_method.name = new_name_token;

            // Substitute generic parameters if needed
            if let Some(concrete_str) = concrete_type_str {
                for p in &mut new_method.parameters {
                    if matches!(p.type_, Type::Struct(_, _)) && p.type_.get_type() == "T" {
                        let dummy_span = crate::lang::code_analysis::text::text_span::TextSpan::new((0, 0), &std::rc::Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string())));
                        let dummy_token = SyntaxToken::new(crate::lang::code_analysis::token::token_kind::TokenKind::DataTypeToken, dummy_span, concrete_str.to_string());
                        p.type_ = match concrete_str {
                            "int" => Type::Integer(dummy_token),
                            "float" => Type::Float(dummy_token),
                            "double" => Type::Double(dummy_token),
                            "string" => Type::String(dummy_token),
                            "bool" => Type::Boolean(dummy_token),
                            _ => Type::Struct(dummy_token, None),
                        };
                    }
                }
                
                if let Some(ret) = &new_method.return_type {
                    if matches!(ret, Type::Struct(_, _)) && ret.get_type() == "T" {
                        let dummy_span = crate::lang::code_analysis::text::text_span::TextSpan::new((0, 0), &std::rc::Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string())));
                        let dummy_token = SyntaxToken::new(crate::lang::code_analysis::token::token_kind::TokenKind::DataTypeToken, dummy_span, concrete_str.to_string());
                        new_method.return_type = Some(match concrete_str {
                            "int" => Type::Integer(dummy_token),
                            "float" => Type::Float(dummy_token),
                            "double" => Type::Double(dummy_token),
                            "string" => Type::String(dummy_token),
                            "bool" => Type::Boolean(dummy_token),
                            _ => Type::Struct(dummy_token, None),
                        });
                    }
                }
            }
            
            // Inject `this` parameter
            let dummy_span = crate::lang::code_analysis::text::text_span::TextSpan::new((0,0), &std::rc::Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string())));
            let this_type_token = SyntaxToken::new(crate::lang::code_analysis::token::token_kind::TokenKind::IdentifierToken, dummy_span.clone(), struct_type_str.to_string());
            let this_type = Type::Struct(this_type_token, None);
            let this_token = SyntaxToken::new(crate::lang::code_analysis::token::token_kind::TokenKind::IdentifierToken, dummy_span, "this".to_string());
            let this_param = crate::lang::code_analysis::syntax::nodes::function::ParameterNode::new(this_token, this_type);
            
            new_method.parameters.insert(0, this_param);
            
            // Allocate the new method in the arena
            let method_ref = self.arena.alloc(new_method);
            self.struct_methods.push(method_ref);
            
            if let Err(e) = self.function_table.add_function(mangled_name.clone(), FunctionTableInfo::from(method_ref)) {
                diagnostics.report_error(e.to_string(), Some(method.name.position.clone()));
            }
        }
    }

    fn analyze_function(&mut self,function:&FunctionNode<'a>, diagnostics: &mut DiagnosticBag) -> Result<Rc<RefCell<SymbolTable>>, ()> {
        let param_table=Rc::new(RefCell::new(self.add_function_param_table(function, diagnostics)?));
        self.analyze_body(function.body,function,Some(&param_table),false, diagnostics)?;
        // check return
        let mut graph=FunctionControlGraph::new(function);
        if let Err(e) = graph.build() {
            diagnostics.report_error(e.to_string(), Some(function.name.position.clone()));
        }
        Ok(param_table.clone())
    }
    fn add_function_param_table(&mut self,function:&FunctionNode<'a>, diagnostics: &mut DiagnosticBag) -> Result<SymbolTable, ()> {
        let mut param_table=SymbolTable::new(None);
        for param in function.parameters.iter() {
            if let Err(e) = param_table.add_symbol(param.name.text.clone(), param.type_.clone()) {
                diagnostics.report_error(e.to_string(), Some(param.name.position.clone()));
            }
        }
        Ok(param_table)
    }

    fn analyze_body(&mut self, body:&[StatementNode<'a>], parent_function:&FunctionNode<'a>,
                    parent_table:Option<&Rc<RefCell<SymbolTable>>>,has_parent_loop:bool, diagnostics: &mut DiagnosticBag) ->Result<(),()> {

        let parent_scope =match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope.clone())));
        if parent_scope.is_some()
        {
            let parent_table=&parent_scope.unwrap();
            (*parent_table).borrow_mut().add_child(symbol_table.clone());
        }
        for statement in body.iter() {
            let clone=&symbol_table.clone();
            self.analyze_statement(statement,parent_function,&clone,has_parent_loop, diagnostics)?;
        }
        Ok(())
    }
    fn analyze_statement(&mut self,statement:&StatementNode<'a>,parent_function:&FunctionNode<'a>,
                         symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        match statement
        {
            StatementNode::Declaration(left, type_annotation, right) =>
                self.analyze_declaration(left, type_annotation, right,parent_function,&symbol_table, diagnostics)?,
            StatementNode::Assignment(left,right) =>
                self.analyze_assignment(left,right,parent_function,&symbol_table, diagnostics)?,
            StatementNode::IndexAssignment(left, index, right) =>
                self.analyze_index_assignment(left, index, right, parent_function, &symbol_table, diagnostics)?,
            StatementNode::MemberAssignment(obj, member, right) =>
                self.analyze_member_assignment(obj, member, right, parent_function, &symbol_table, diagnostics)?,
            StatementNode::IfElse(condition,if_body,
                                  else_if,else_body)=>
                self.analyze_if_else(condition,if_body,
                                     else_if,else_body,parent_function,&symbol_table,has_parent_while, diagnostics)?,
            StatementNode::Return(expression) =>
                self.analyze_return(expression,parent_function,&symbol_table, diagnostics)?,
            StatementNode::While(condition,body) =>
                self.analyze_while(condition,body,parent_function,&symbol_table, diagnostics)?,
            StatementNode::For(init,condition,increment,body) =>
                self.analyze_for(init,condition,increment,body,parent_function,&symbol_table, diagnostics)?,
            StatementNode::Break=>
                self.analyze_break(parent_function,has_parent_while, diagnostics)?,
            StatementNode::Continue=>
                self.analyze_continue(parent_function,has_parent_while, diagnostics)?,
            StatementNode::FunctionInvocation(name, generic_args, params) =>
                {self.analyze_function_call(name, generic_args, params,parent_function,symbol_table, diagnostics)?;},
            StatementNode::MethodInvocation(obj, method, generic_args, params) =>
                {self.analyze_method_call(obj, method, generic_args, params, parent_function, symbol_table, diagnostics)?;},
        };
        Ok(())
    }
    fn analyze_function_call(&mut self,name:&SyntaxToken,generic_args: &Option<Vec<Type>>,params:&Vec<ExpressionNode<'a>>,
                                   parent_function:&FunctionNode<'a>,
                                   symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        let mut function_name=name.text.clone();
        let mut params_types=vec![];
        for param in params.iter() {
            params_types.push(self.analyze_expression(param,parent_function,symbol_table, diagnostics)?.get_type());
        }

        // Monomorphization logic
        if self.generic_functions.contains_key(&function_name) {
            let concrete_type_str = if let Some(generics) = generic_args {
                if !generics.is_empty() {
                    generics[0].get_type()
                } else {
                    "void".to_string()
                }
            } else if !params_types.is_empty() {
                params_types[0].clone()
            } else {
                "void".to_string()
            };

            let mangled_name = format!("{}_{}", function_name, concrete_type_str);
            
            // If we haven't instantiated this concrete function yet, do it now
            if self.function_table.get_function(&mangled_name).is_err() {
                let template = *self.generic_functions.get(&function_name).unwrap();
                
                self.instantiated_generics.insert(mangled_name.clone(), (concrete_type_str.clone(), template));
                
                let mut new_params = Vec::new();
                for p in &template.parameters {
                    let mut p_type = p.type_.clone();
                    if matches!(p_type, Type::Struct(_, _)) && p_type.get_type() == "T" {
                        let dummy_span = crate::lang::code_analysis::text::text_span::TextSpan::new((0, 0), &Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string())));
                        let dummy_token = SyntaxToken::new(TokenKind::DataTypeToken, dummy_span, concrete_type_str.clone());
                        p_type = match concrete_type_str.as_str() {
                            "int" => Type::Integer(dummy_token),
                            "float" => Type::Float(dummy_token),
                            "double" => Type::Double(dummy_token),
                            "string" => Type::String(dummy_token),
                            "bool" => Type::Boolean(dummy_token),
                            _ => Type::Void,
                        };
                    }
                    new_params.push(p_type);
                }
                
                let return_type = match &template.return_type {
                    Some(Type::Struct(t, _)) if t.text == "T" => {
                        let dummy_span = crate::lang::code_analysis::text::text_span::TextSpan::new((0, 0), &Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string())));
                        let dummy_token = SyntaxToken::new(TokenKind::DataTypeToken, dummy_span, concrete_type_str.clone());
                        Some(match concrete_type_str.as_str() {
                            "int" => Type::Integer(dummy_token),
                            "float" => Type::Float(dummy_token),
                            "double" => Type::Double(dummy_token),
                            "string" => Type::String(dummy_token),
                            "bool" => Type::Boolean(dummy_token),
                            _ => Type::Void,
                        })
                    },
                    other => other.clone()
                };

                let info = FunctionTableInfo {
                    name: mangled_name.clone(),
                    parameters: new_params.iter().map(|p| p.get_type()).collect(),
                    return_type,
                };
                
                let _ = self.function_table.add_function(mangled_name.clone(), info);
            }
            function_name = mangled_name;
        }

        let store_sig = match self.function_table.get_function(&function_name) {
            Ok(sig) => sig,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(name.position.clone()));
                return Ok(Type::Void);
            }
        };

        if store_sig.parameters.len()!=params_types.len() {
            diagnostics.report_error(format!("Function {} has {} params but {} params are given",
                                                           function_name,store_sig.parameters.len(),params_types.len()), Some(name.position.clone()));
            return Ok(Type::Void);
        }

        for i in 0..params_types.len() {
            if store_sig.parameters.get(i)!=params_types.get(i) {
                diagnostics.report_error(format!("Function {} has param {} of type {:?} but param {} of type {:?} is given",
                                                               function_name,i,store_sig.parameters.get(i),i,params_types[i]), Some(name.position.clone()));
            }
        }

        //let r_type=&store_sig.return_type;
        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }
    fn analyze_method_call(&mut self, obj: &ExpressionNode<'a>, method: &SyntaxToken, generic_args: &Option<Vec<Type>>, params: &Vec<ExpressionNode<'a>>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;
        
        let struct_name = match obj_type {
            Type::Struct(token, gen_args) => {
                let mut n = token.text.clone();
                if let Some(args) = gen_args {
                    if !args.is_empty() {
                        n = format!("{}_{}", n, args[0].get_type());
                    }
                }
                n
            },
            Type::Nullable(ref inner) => {
                if let Type::Struct(token, gen_args) = &**inner {
                    let mut n = token.text.clone();
                    if let Some(args) = gen_args {
                        if !args.is_empty() {
                            n = format!("{}_{}", n, args[0].get_type());
                        }
                    }
                    n
                } else {
                    diagnostics.report_error(format!("Cannot call method on non-struct type {}", obj_type.get_type()), Some(method.position.clone()));
                    return Ok(Type::Void);
                }
            },
            _ => {
                diagnostics.report_error(format!("Cannot call method on non-struct type {}", obj_type.get_type()), Some(method.position.clone()));
                return Ok(Type::Void);
            }
        };

        self.ensure_struct_instantiated(&struct_name, &method.position, diagnostics);
        
        let mangled_name = format!("{}_{}", struct_name, method.text);
        
        let store_sig = match self.function_table.get_function(&mangled_name) {
            Ok(s) => s.clone(),
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(method.position.clone()));
                return Ok(Type::Void);
            }
        };

        let mut expected_params = store_sig.parameters.clone();
        
        // Remove 'this' from the expected params check since we supply it implicitly
        if !expected_params.is_empty() {
            expected_params.remove(0);
        }

        if expected_params.len() != params.len() {
            diagnostics.report_error(format!("function {} expects {} parameters, got {}", mangled_name, expected_params.len(), params.len()), Some(method.position.clone()));
            return Ok(store_sig.return_type.unwrap_or(Type::Void));
        }

        for (i, param) in params.iter().enumerate() {
            let param_type = self.analyze_expression(param, parent_function, symbol_table, diagnostics)?;
            let expected_type_str = &expected_params[i];

            if expected_type_str == "int" && param_type.get_type() == "float" || expected_type_str == "float" && param_type.get_type() == "int" || expected_type_str == "double" && param_type.get_type() == "int" || expected_type_str == "int" && param_type.get_type() == "double" || expected_type_str == "float" && param_type.get_type() == "double" || expected_type_str == "double" && param_type.get_type() == "float" {
                continue;
            }

            if param_type.get_type() != *expected_type_str {
                diagnostics.report_error(format!("function {} expects parameter {} to be {}, got {}", mangled_name, i + 1, expected_type_str, param_type.get_type()), Some(method.position.clone()));
            }
        }

        Ok(store_sig.return_type.unwrap_or(Type::Void))
    }

    fn analyze_break(&mut self,parent_function:&FunctionNode<'a>,has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        if !has_parent_while {
            diagnostics.report_error(
                                  format!("Break statement is not in a loop in function {}",parent_function.name.text), Some(parent_function.name.position.clone()));
        }
        Ok(())
    }
    fn analyze_continue(&mut self,parent_function:&FunctionNode<'a>,has_parent_while:bool, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        if !has_parent_while {
            diagnostics.report_error(
                                  format!("Continue statement is not in a loop in function {}",parent_function.name.text), Some(parent_function.name.position.clone()));
        }
        Ok(())
    }
    fn analyze_while(&mut self,condition:&ExpressionNode<'a>,body:&[StatementNode<'a>],
                     parent_function:&FunctionNode<'a>,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        let cond_type = self.analyze_expression(condition,parent_function,symbol_table, diagnostics)?;
        if cond_type.get_type() != "bool" {
            diagnostics.report_error(format!("while condition must be bool, got {}", cond_type.get_type()), None);
        }
        self.analyze_body(body,parent_function,Some(symbol_table),true, diagnostics)?;
        Ok(())
    }
    fn analyze_for(&mut self,init:&Option<&'a StatementNode<'a>>,condition:&Option<ExpressionNode<'a>>,
                   increment:&Option<&'a StatementNode<'a>>,body:&[StatementNode<'a>],
                   parent_function:&FunctionNode<'a>,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()>
    {
        let for_scope = Rc::new(RefCell::new(SymbolTable::new(Some(symbol_table.clone()))));
        (*symbol_table).borrow_mut().add_child(for_scope.clone());

        if let Some(init_stmt) = init {
            self.analyze_statement(init_stmt, parent_function, &for_scope, false, diagnostics)?;
        }
        if let Some(cond_expr) = condition {
            let cond_type = self.analyze_expression(cond_expr, parent_function, &for_scope, diagnostics)?;
            if cond_type.get_type() != "bool" {
                diagnostics.report_error(format!("for condition must be bool, got {}", cond_type.get_type()), None);
            }
        }
        if let Some(inc_stmt) = increment {
            self.analyze_statement(inc_stmt, parent_function, &for_scope, false, diagnostics)?;
        }
        self.analyze_body(body, parent_function, Some(&for_scope), true, diagnostics)?;
        Ok(())
    }
    ///return type is returned currently int and float supported
    fn analyze_declaration(&mut self,left:&SyntaxToken, type_annotation: &Option<Type>, right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                           symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        //return right type
        let right_type=self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;
        
        let var_type = if let Some(t) = type_annotation {
            self.compare_data_type(t, &right_type, &left.position, diagnostics)?;
            t.clone()
        } else {
            right_type.clone()
        };
        
        if let Err(e) = (*symbol_table).as_ref().borrow_mut().add_symbol(left.text.clone(), var_type) {
            diagnostics.report_error(e.to_string(), Some(left.position.clone()));
        }
        Ok(())
    }
    fn analyze_assignment(&mut self,left:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                          symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        let r=self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;
        let l = match (*symbol_table).as_ref().borrow().get_symbol(left.clone()) {
            Ok(sym) => sym,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(left.position.clone()));
                return Ok(());
            }
        };
        self.compare_data_type(&l,&r,&left.position, diagnostics)?;
        Ok(())
    }
    
    fn analyze_index_assignment(&mut self, arr: &ExpressionNode<'a>, index: &ExpressionNode<'a>, right: &ExpressionNode<'a>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let array_type = self.analyze_expression(arr, parent_function, symbol_table, diagnostics)?;

        let inner_type = match array_type {
            Type::Array(inner) => *inner,
            _ => {
                diagnostics.report_error(format!("Cannot index into non-array type {}", array_type.get_type()), None);
                return Ok(());
            }
        };

        let index_type = self.analyze_expression(index, parent_function, symbol_table, diagnostics)?;
        if index_type.get_type() != "int" {
            diagnostics.report_error(format!("Array index must be of type int, got {}", index_type.get_type()), None);
        }

        let right_type = self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&inner_type, &right_type, &SyntaxToken::new(TokenKind::BadToken, crate::lang::code_analysis::text::text_span::TextSpan::new((0,0), &std::rc::Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string()))), "".to_string()).position, diagnostics)?;
        
        Ok(())
    }

    fn analyze_member_assignment(&mut self, obj: &ExpressionNode<'a>, member: &SyntaxToken, right: &ExpressionNode<'a>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;
        
        let struct_name = match obj_type {
            Type::Struct(token, generic_args) => {
                let mut n = token.text.clone();
                if let Some(args) = generic_args {
                    if !args.is_empty() {
                        n = format!("{}_{}", n, args[0].get_type());
                    }
                }
                n
            },
            Type::Nullable(ref inner) => {
                if let Type::Struct(token, generic_args) = &**inner {
                    let mut n = token.text.clone();
                    if let Some(args) = generic_args {
                        if !args.is_empty() {
                            n = format!("{}_{}", n, args[0].get_type());
                        }
                    }
                    n
                } else {
                    diagnostics.report_error(format!("Cannot access member of non-struct type {}", obj_type.get_type()), Some(member.position.clone()));
                    return Ok(());
                }
            },
            _ => {
                diagnostics.report_error(format!("Cannot access member of non-struct type {}", obj_type.get_type()), Some(member.position.clone()));
                return Ok(());
            }
        };

        self.ensure_struct_instantiated(&struct_name, &member.position, diagnostics);

        let field_type = {
            let struct_info = match self.struct_table.get_struct(&struct_name) {
                Some(info) => info,
                None => {
                    diagnostics.report_error(format!("Struct '{}' not found", struct_name), Some(member.position.clone()));
                    return Ok(());
                }
            };

            match struct_info.fields.get(&member.text) {
                Some(info) => info.type_.clone(),
                None => {
                    diagnostics.report_error(format!("Field '{}' not found in struct '{}'", member.text, struct_name), Some(member.position.clone()));
                    return Ok(());
                }
            }
        };

        let right_type = self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&field_type, &right_type, &member.position, diagnostics)?;
        
        Ok(())
    }
    fn analyze_expression(&mut self,expression:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                          symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        return match expression
        {
            ExpressionNode::Literal(number) =>
                Ok(number.clone()),
            ExpressionNode::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    diagnostics.report_error("Empty array literals are not supported yet".to_string(), None);
                    return Ok(Type::Array(Box::new(Type::Void)));
                }
                
                let first_type = self.analyze_expression(&elements[0], parent_function, symbol_table, diagnostics)?;
                
                for i in 1..elements.len() {
                    let element_type = self.analyze_expression(&elements[i], parent_function, symbol_table, diagnostics)?;
                    // We can't easily get the position of the element here without changing AST, so we pass None
                    self.compare_data_type(&first_type, &element_type, &SyntaxToken::new(TokenKind::BadToken, crate::lang::code_analysis::text::text_span::TextSpan::new((0,0), &std::rc::Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string()))), "".to_string()).position, diagnostics)?;
                }
                
                Ok(Type::Array(Box::new(first_type)))
            },
            ExpressionNode::IndexAccess(array_expr, index_expr) => {
                let array_type = self.analyze_expression(array_expr, parent_function, symbol_table, diagnostics)?;
                let inner_type = match array_type {
                    Type::Array(inner) => *inner,
                    _ => {
                        diagnostics.report_error(format!("Cannot index into non-array type {}", array_type.get_type()), None);
                        Type::Void
                    }
                };
                
                let index_type = self.analyze_expression(index_expr, parent_function, symbol_table, diagnostics)?;
                if index_type.get_type() != "int" {
                    diagnostics.report_error(format!("Array index must be of type int, got {}", index_type.get_type()), None);
                }
                
                Ok(inner_type)
            },
            ExpressionNode::Unary(opr,right)=> {
                let right_type = self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;
                match opr.kind {
                    TokenKind::BangToken => {
                        if right_type.get_type() != "bool" {
                            diagnostics.report_error(format!("! operator requires bool, got {}", right_type.get_type()), Some(opr.position.clone()));
                        }
                        Ok(Type::Boolean(opr.clone()))
                    },
                    TokenKind::PlusToken | TokenKind::MinusToken => {
                        if right_type.get_type() != "int" && right_type.get_type() != "float" {
                            diagnostics.report_error(format!("unary +/- requires int or float, got {}", right_type.get_type()), Some(opr.position.clone()));
                        }
                        Ok(right_type)
                    },
                    _ => {
                        diagnostics.report_error(format!("unknown unary operator {}", opr.text), Some(opr.position.clone()));
                        Ok(right_type)
                    }
                }
            },
            ExpressionNode::Binary(left,opr,right)=>
                Ok(self.analyze_binary_expression(left,opr,right,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::Identifier(id)=>
                Ok(self.analyze_identifier(id,symbol_table, diagnostics)?),
            ExpressionNode::FunctionCall(name,generic_args,params)=>
                Ok(self.analyze_function_call(name,generic_args,params,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::IsExpression(left, right_type) => {
                let left_type = self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                
                // Get the position from the type if possible, or just create an empty span
                let empty_span = crate::lang::code_analysis::text::text_span::TextSpan::new((0,0), &Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string())));
                let dummy_token = SyntaxToken::new(TokenKind::BooleanToken, empty_span, "true".to_string());
                Ok(Type::Boolean(dummy_token))
            },
            ExpressionNode::Parenthesized(expr)=>
                Ok(self.analyze_expression(expr,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::StructInstantiation(name, generic_args, fields) => {
                let mut struct_name = name.text.clone();
                if let Some(args) = generic_args {
                    if !args.is_empty() {
                        struct_name = format!("{}_{}", struct_name, args[0].get_type());
                    }
                }
                
                // Monomorphize generic struct if needed
                self.ensure_struct_instantiated(&struct_name, &name.position, diagnostics);
                
                let struct_info = match self.struct_table.get_struct(&struct_name) {
                    Some(info) => info.clone(),
                    None => {
                        diagnostics.report_error(format!("Struct '{}' not found", struct_name), Some(name.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                // Check that all fields are provided and types match
                let mut provided_fields = std::collections::HashSet::new();
                for (field_name, field_expr) in fields {
                    provided_fields.insert(field_name.text.clone());
                    
                    let field_info = match struct_info.fields.get(&field_name.text) {
                        Some(info) => info,
                        None => {
                            diagnostics.report_error(format!("Field '{}' not found in struct '{}'", field_name.text, struct_name), Some(field_name.position.clone()));
                            continue;
                        }
                    };

                    let expr_type = self.analyze_expression(field_expr, parent_function, symbol_table, diagnostics)?;
                    self.compare_data_type(&field_info.type_, &expr_type, &field_name.position, diagnostics)?;
                }

                // Check for missing fields
                for expected_field in struct_info.fields.keys() {
                    if !provided_fields.contains(expected_field) {
                        diagnostics.report_error(format!("Missing field '{}' in struct instantiation of '{}'", expected_field, struct_name), Some(name.position.clone()));
                    }
                }

                let mut dummy_token = name.clone();
                dummy_token.text = struct_name.clone();
                Ok(Type::Struct(dummy_token, None))
            },
            ExpressionNode::MemberAccess(obj, member) => {
                let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;
                
                let struct_name = match obj_type {
                    Type::Struct(token, generic_args) => {
                        let mut n = token.text.clone();
                        if let Some(args) = generic_args {
                            if !args.is_empty() {
                                n = format!("{}_{}", n, args[0].get_type());
                            }
                        }
                        n
                    },
                    Type::Nullable(ref inner) => {
                        if let Type::Struct(token, generic_args) = &**inner {
                            let mut n = token.text.clone();
                            if let Some(args) = generic_args {
                                if !args.is_empty() {
                                    n = format!("{}_{}", n, args[0].get_type());
                                }
                            }
                            n
                        } else {
                            diagnostics.report_error(format!("Cannot access member of non-struct type {}", obj_type.get_type()), Some(member.position.clone()));
                            return Ok(Type::Void);
                        }
                    },
                    _ => {
                        diagnostics.report_error(format!("Cannot access member of non-struct type {}", obj_type.get_type()), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                self.ensure_struct_instantiated(&struct_name, &member.position, diagnostics);

                let struct_info = match self.struct_table.get_struct(&struct_name) {
                    Some(info) => info,
                    None => {
                        diagnostics.report_error(format!("Struct '{}' not found", struct_name), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                let field_info = match struct_info.fields.get(&member.text) {
                    Some(info) => info,
                    None => {
                        diagnostics.report_error(format!("Field '{}' not found in struct '{}'", member.text, struct_name), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                Ok(field_info.type_.clone())
            },
            ExpressionNode::Cast(target_type, expr) => {
                let expr_type = self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?;
                
                let target_type_str = target_type.get_type();
                let expr_type_str = expr_type.get_type();
                
                // If target type is a struct, ensure it's instantiated
                if target_type_str != "int" && target_type_str != "float" && target_type_str != "double" && target_type_str != "bool" && target_type_str != "string" && target_type_str != "void" {
                    let base_type_str = if target_type_str.ends_with("[]") {
                        &target_type_str[..target_type_str.len() - 2]
                    } else if target_type_str.ends_with("?") {
                        &target_type_str[..target_type_str.len() - 1]
                    } else {
                        &target_type_str
                    };
                    let pos = SyntaxToken::new(TokenKind::BadToken, crate::lang::code_analysis::text::text_span::TextSpan::new((0,0), &std::rc::Rc::new(crate::lang::code_analysis::text::line_text::LineText::new("".to_string()))), "".to_string()).position;
                    self.ensure_struct_instantiated(base_type_str, &pos, diagnostics);
                }

                // Allow int <-> float casts
                if (target_type_str == "int" && expr_type_str == "float") ||
                   (target_type_str == "float" && expr_type_str == "int") ||
                   (target_type_str == "double" && expr_type_str == "int") ||
                   (target_type_str == "int" && expr_type_str == "double") ||
                   (target_type_str == "float" && expr_type_str == "double") ||
                   (target_type_str == "double" && expr_type_str == "float") {
                    Ok(target_type.clone())
                } else if target_type_str == expr_type_str {
                    Ok(target_type.clone())
                } else if expr_type_str == "int" && (self.struct_table.get_struct(&target_type_str).is_some() || target_type_str.ends_with("[]") || target_type_str.ends_with("?")) {
                    // Allow casting int to pointer types (for null pointers)
                    Ok(target_type.clone())
                } else {
                    diagnostics.report_error(format!("Cannot cast from {} to {}", expr_type_str, target_type_str), None);
                    Ok(target_type.clone())
                }
            },
            ExpressionNode::MethodCall(obj, method, generic_args, params) => self.analyze_method_call(obj, method, generic_args, params, parent_function, symbol_table, diagnostics),
        }
    }
    fn analyze_binary_expression(&mut self,left:&ExpressionNode<'a>,opr:&SyntaxToken,right:&ExpressionNode<'a>,parent_function:&FunctionNode<'a>,
                                 symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        let left_value = self.analyze_expression(left,parent_function,symbol_table, diagnostics)?;
        let right_value = self.analyze_expression(right,parent_function,symbol_table, diagnostics)?;

        self.compare_data_type(&left_value,&right_value,&opr.position, diagnostics)?;
        match (&left_value,&opr.kind) {
          (Type::String(_),TokenKind::PlusToken)=> {}
          (Type::String(_),_)=> {
              diagnostics.report_error(format!("Cannot perform operation {} on string",opr.text), Some(opr.position.clone()));
          }
            (_,_)=>{}
        };
        
        match opr.kind {
            TokenKind::EqualEqualToken | TokenKind::NotEqualToken |
            TokenKind::GreaterThanToken | TokenKind::GreaterThanEqualToken |
            TokenKind::SmallerThanToken | TokenKind::SmallerThanEqualToken |
            TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken => {
                return Ok(Type::Boolean(opr.clone()));
            },
            _ => return Ok(left_value)
        }
    }
    fn compare_data_type(&mut self, left:&Type, right:&Type, position:&TextSpan, diagnostics: &mut DiagnosticBag) ->Result<(),()> {
        if left.get_type() == right.get_type() {
            return Ok(())
        }
        
        // Handle nullable types
        if let Type::Nullable(inner) = left {
            if let Type::Nullable(inner_right) = right {
                if inner.get_type() == inner_right.get_type() {
                    return Ok(());
                }
            } else if inner.get_type() == right.get_type() {
                // Allow assigning non-nullable to nullable
                return Ok(());
            } 
            
            // If right is Nullable(Void) i.e. `null` literal
            if right.get_type() == "void?" {
                return Ok(());
            }
        } else if let Type::Nullable(inner_right) = right {
            // Allow assigning non-nullable to nullable (wait, this is if left is NOT nullable, but right IS nullable? We shouldn't allow that unless right is null literal being assigned to nullable? No, left is not nullable. We shouldn't allow right to be nullable unless it's a cast or something. But wait, if right is `null` literal, and left is nullable, it's handled above.)
            // Wait, what if left is `Node?` and right is `void?`? Handled above.
            // What if left is `Node` and right is `void?`? Error.
            // What if left is `Node` and right is `Node?`? Error.
        }
        
        // Allow comparing nullable with null literal
        if (left.get_type().ends_with("?") || self.is_reference_type(&left.get_type())) && right.get_type() == "void?" {
            return Ok(());
        }
        if (right.get_type().ends_with("?") || self.is_reference_type(&right.get_type())) && left.get_type() == "void?" {
            return Ok(());
        }
        
        diagnostics.report_error(format!("cannot convert from {} to {} at {}",
                       left.get_type(),right.get_type(),position.get_point_str()), Some(position.clone()));
        Ok(())
    }

    pub fn is_reference_type(&self, type_name: &str) -> bool {
        let base_name = if type_name.ends_with("?") {
            &type_name[..type_name.len() - 1]
        } else {
            type_name
        };
        
        if base_name == "string" || base_name.ends_with("[]") || self.struct_table.get_struct(base_name).is_some() {
            return true;
        }

        let parts: Vec<&str> = base_name.split('_').collect();
        if parts.len() >= 2 && self.generic_structs.contains_key(parts[0]) {
            return true;
        }

        false
    }
    fn analyze_identifier(&mut self,id:&SyntaxToken,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        let r= match (*symbol_table).as_ref().borrow_mut().get_symbol(id.clone()) {
            Ok(t) => t,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(id.position.clone()));
                Type::Void
            }
        };
        Ok(r)
    }

    fn analyze_if_else(&mut self, condition:&ExpressionNode<'a>, if_body:&[StatementNode<'a>],
                       else_if:&Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>,
                       else_body: &Option<&'a [StatementNode<'a>]>,
                       parent_function:&FunctionNode<'a>, symbol_table:&Rc<RefCell<SymbolTable>>,has_parent_while:bool, diagnostics: &mut DiagnosticBag) ->
    Result<(),()>
    {
        // Check for constant expression from `is`
        let mut is_constant_true = false;
        let mut is_constant_false = false;
        
        if let ExpressionNode::IsExpression(left, right_type) = condition {
            let left_t = self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
            if left_t.get_type() == right_type.get_type() {
                is_constant_true = true;
            } else {
                is_constant_false = true;
            }
        }
        
        if !is_constant_false {
            //if condition
            let cond_type = self.analyze_expression(condition,parent_function,symbol_table, diagnostics)?;
            if cond_type.get_type() != "bool" {
                diagnostics.report_error(format!("if condition must be bool, got {}", cond_type.get_type()), None);
            }
            //if body
            self.analyze_body(if_body,parent_function,Some(symbol_table),has_parent_while, diagnostics)?;
        }
        
        if is_constant_true {
            return Ok(());
        }

        //else if block
        for i in else_if.iter()
        {
            let mut elif_constant_true = false;
            let mut elif_constant_false = false;
            if let ExpressionNode::IsExpression(left, right_type) = &i.0 {
                let left_t = self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                if left_t.get_type() == right_type.get_type() {
                    elif_constant_true = true;
                } else {
                    elif_constant_false = true;
                }
            }

            if !elif_constant_false {
                let elif_cond_type = self.analyze_expression(&i.0,parent_function,symbol_table, diagnostics)?;
                if elif_cond_type.get_type() != "bool" {
                    diagnostics.report_error(format!("else if condition must be bool, got {}", elif_cond_type.get_type()), None);
                }
                self.analyze_body(&i.1,parent_function,Some(symbol_table),has_parent_while, diagnostics)?;
            }
            
            if elif_constant_true {
                return Ok(());
            }
        }
        
        match else_body
        {
            Some(body)=>self.analyze_body(body,parent_function,Some(symbol_table),has_parent_while, diagnostics)?,
            None=>()
        }
        Ok(())
    }
    fn analyze_return(&mut self,expression:&Option<ExpressionNode<'a>>,parent_function:&FunctionNode<'a>,
                      symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<(),()> {
        match (expression,&parent_function.return_type)
        {
            (Some(expression),&Some(ref return_type))=>
            {
                let r=self.analyze_expression(expression,parent_function,symbol_table, diagnostics)?;
                self.compare_data_type(return_type, &r, &parent_function.name.position, diagnostics)?;
            },
            (None,&Some(_))=> {
                diagnostics.report_error(format!("return type mismatch at  {}",parent_function.name.position.get_point_str()), Some(parent_function.name.position.clone()));
            },
            (Some(_),&None)=> {
                diagnostics.report_error(format!("return type mismatch at {}",parent_function.name.position.get_point_str()), Some(parent_function.name.position.clone()));
            },
            (None,&None)=>()
        };
        Ok(())
    }

}

#[cfg(test)]
#[path = "tests/analyzer_tests.rs"]
mod tests;