use bumpalo::Bump;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::lang::code_analysis::syntax::nodes::{ExpressionNode, FunctionNode, Type, ProgramNode, StatementNode};
use crate::lang::code_analysis::syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use crate::lang::code_analysis::syntax::nodes::function::ParameterNode;
use crate::lang::code_analysis::syntax::nodes::types::{mangle_generic, strip_array, strip_nullable};
use crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree;
use crate::lang::code_analysis::text::line_text::LineText;
use crate::lang::code_analysis::text::text_span::TextSpan;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::semantic_analysis::function_control_flow::FunctionControlGraph;
use crate::lang::semantic_analysis::function_table::{FunctionTable, FunctionTableInfo};
use crate::lang::semantic_analysis::symbol_table::SymbolTable;
use crate::lang::semantic_analysis::struct_table::StructTable;
use crate::lang::diagnostics::DiagnosticBag;

/// Converts an AST node's `Rc<str>` source-file tag into the `String` form stored on the
/// diagnostic bag (used to attribute each semantic error to its originating file).
fn file_path_string(file_path: &Option<Rc<str>>) -> Option<String> {
    file_path.as_ref().map(|p| p.to_string())
}

/// An empty source span, used for diagnostics on synthesized nodes that have no real
/// position in the user's source (e.g. array element type mismatches).
fn empty_span() -> TextSpan {
    TextSpan::new((0, 0), &Rc::new(LineText::new(String::new())))
}

/// Creates a token with an empty source span, used when the analyzer synthesizes
/// AST nodes (injected `this` parameters, monomorphized generic types, etc.).
fn synthetic_token(kind: TokenKind, text: &str) -> SyntaxToken {
    SyntaxToken::new(kind, empty_span(), text.to_string())
}

/// Builds the generic substitution bindings (parameter name -> concrete type name) by
/// zipping declared generic parameters with the supplied concrete arguments. Extra
/// parameters or arguments beyond the common length are ignored (arity is validated
/// separately so a clear diagnostic is produced).
fn generic_bindings(params: &[SyntaxToken], args: &[Type]) -> Vec<(String, String)> {
    params.iter()
        .zip(args.iter())
        .map(|(param, arg)| (param.text.clone(), arg.get_type()))
        .collect()
}

/// Looks up the concrete type bound to a generic parameter name, if any.
fn lookup_binding(bindings: &[(String, String)], name: &str) -> Option<String> {
    bindings.iter().find(|(param, _)| param == name).map(|(_, concrete)| concrete.clone())
}

/// Builds a mangled function name by appending each concrete type from the bindings in order,
/// e.g. base `swap` with bindings `[(T,int),(V,string)]` becomes `swap_int_string`.
fn mangle_bindings(base: &str, bindings: &[(String, String)]) -> String {
    let mut name = base.to_string();
    for (_, concrete) in bindings {
        name.push('_');
        name.push_str(concrete);
    }
    name
}

/// Rewrites a field type token that refers to a generic parameter (e.g. `T`, `T[]`, `T?`)
/// into its concrete form, preserving the array/nullable suffix. Tokens that do not name a
/// generic parameter are returned unchanged.
fn substitute_generic_token(token: &SyntaxToken, bindings: &[(String, String)]) -> SyntaxToken {
    let mut result = token.clone();
    let (base, suffix) = if let Some(base) = token.text.strip_suffix("[]") {
        (base, "[]")
    } else if let Some(base) = token.text.strip_suffix('?') {
        (base, "?")
    } else {
        (token.text.as_str(), "")
    };
    if let Some(concrete) = lookup_binding(bindings, base) {
        result.text = format!("{}{}", concrete, suffix);
    }
    result
}

/// Maps each generic parameter name to the concrete type bound to it for one monomorphization.
pub type GenericBindings = Vec<(String, String)>;

pub struct SemanticInfo<'a>
{
    pub hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
    pub struct_table: &'a StructTable,
    pub instantiated_generics: HashMap<String, (GenericBindings, &'a FunctionNode<'a>)>,
    pub struct_methods: Vec<&'a FunctionNode<'a>>,
}

impl<'a> SemanticInfo<'a> {
    pub fn new(hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>, function_table: &'a FunctionTable, struct_table: &'a StructTable, instantiated_generics: HashMap<String, (GenericBindings, &'a FunctionNode<'a>)>, struct_methods: Vec<&'a FunctionNode<'a>>) -> SemanticInfo<'a>
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


pub struct Analyzer<'a> {
    syntax_tree:&'a SyntaxTree<'a>,
    function_table:FunctionTable,
    struct_table:StructTable,
    arena: &'a Bump,
    generic_functions: HashMap<String, &'a FunctionNode<'a>>,
    instantiated_generics: HashMap<String, (GenericBindings, &'a FunctionNode<'a>)>,
    generic_structs: HashMap<String, &'a crate::lang::code_analysis::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    struct_methods: Vec<&'a FunctionNode<'a>>,
}
impl<'a> Analyzer<'a> {
    pub fn new(tree: &'a SyntaxTree<'a>, arena: &'a Bump) -> Self {
        Self { syntax_tree:tree, function_table: FunctionTable::new(), struct_table: StructTable::new(), arena, generic_functions: HashMap::new(), instantiated_generics: HashMap::new(), generic_structs: HashMap::new(), struct_methods: Vec::new() }
    }
    pub fn analyze(&mut self, diagnostics: &mut DiagnosticBag) -> Result<SemanticInfo<'_>, ()> {
        let pgm= self.syntax_tree.get_root();
        self.analyze_pgm(pgm, diagnostics)
    }

    /// Builds a concrete `Type` from a type name, used when substituting a generic
    /// parameter `T` with the concrete type chosen at the call/instantiation site.
    fn concrete_type_from_str(name: &str) -> Type {
        let token = synthetic_token(TokenKind::DataTypeToken, name);
        match name {
            "int" => Type::Integer(token),
            "float" => Type::Float(token),
            "double" => Type::Double(token),
            "string" => Type::String(token),
            "bool" => Type::Boolean(token),
            _ => Type::Struct(token, None),
        }
    }

    /// If `ty` is a struct (or nullable struct), returns its base name and the list of
    /// concrete generic type arguments (empty for non-generic structs). Returns `None`
    /// for any non-struct type. Does NOT recurse into arrays (a method/member access on an
    /// array is invalid and must surface as an error).
    fn resolve_struct_parts(ty: &Type) -> Option<(String, Vec<Type>)> {
        match ty {
            Type::Struct(token, args) => Some((token.text.clone(), args.clone().unwrap_or_default())),
            Type::Nullable(inner) => Self::resolve_struct_parts(inner),
            _ => None,
        }
    }

    /// If `ty` is a struct (or nullable struct), returns its mangled type name
    /// (e.g. `Node`, `Box_int`, `Pair_int_string`). Returns `None` for any non-struct type.
    fn resolve_struct_name(ty: &Type) -> Option<String> {
        Self::resolve_struct_parts(ty).map(|(base, args)| mangle_generic(&base, &args))
    }

    /// Splits a mangled generic struct name (e.g. `Box_int`) into its base name and
    /// concrete type argument, choosing the split so the base is a registered generic
    /// struct. This tolerates underscores in both the base name and the concrete type.
    fn demangle_generic_struct(&self, mangled: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = mangled.split('_').collect();
        for split in 1..parts.len() {
            let base = parts[..split].join("_");
            if self.generic_structs.contains_key(&base) {
                return Some((base, parts[split..].join("_")));
            }
        }
        None
    }
    fn analyze_pgm(&mut self,node:&'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) -> Result<SemanticInfo<'_>, ()> {
        let mut symbol_table_map = HashMap::new();

        self.register_structs(node, diagnostics);
        self.register_functions(node, diagnostics);
        self.analyze_function_bodies(node, &mut symbol_table_map, diagnostics)?;
        self.analyze_instantiated_generics(&mut symbol_table_map, diagnostics)?;
        self.analyze_struct_method_bodies(&mut symbol_table_map, diagnostics)?;

        Ok(SemanticInfo::new(symbol_table_map, &self.function_table, &self.struct_table, self.instantiated_generics.clone(), self.struct_methods.clone()))
    }

    /// Pass 0: register every (non-generic) struct and its methods; stash generic templates.
    fn register_structs(&mut self, node: &'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) {
        for struct_decl in node.structs.iter() {
            diagnostics.file_path = file_path_string(&struct_decl.file_path);
            if struct_decl.generic_parameters.is_some() {
                self.generic_structs.insert(struct_decl.name.text.clone(), struct_decl);
                continue;
            }
            if let Err(e) = self.struct_table.add_struct(struct_decl) {
                diagnostics.report_error(e, Some(struct_decl.name.position.clone()));
            }
            self.register_struct_methods(struct_decl, &struct_decl.name.text, &[], diagnostics);
        }
    }

    /// Pass 1: register every (non-generic) function signature; stash generic templates.
    fn register_functions(&mut self, node: &'a ProgramNode<'a>, diagnostics: &mut DiagnosticBag) {
        for function in node.functions.iter() {
            diagnostics.file_path = file_path_string(&function.file_path);
            if function.generic_parameters.is_some() {
                self.generic_functions.insert(function.name.text.clone(), function);
                continue;
            }
            if function.is_exported {
                self.check_export_visibility(function, diagnostics);
            }
            if let Err(e) = self.function_table.add_function(function.name.text.clone(), FunctionTableInfo::from(function)) {
                diagnostics.report_error(e.to_string(), Some(function.name.position.clone()));
            }
        }
    }

    /// Ensures an exported function does not leak a non-exported struct through its signature.
    fn check_export_visibility(&self, function: &FunctionNode<'a>, diagnostics: &mut DiagnosticBag) {
        let signature_types = function.return_type.iter()
            .chain(function.parameters.iter().map(|p| &p.type_));
        for type_to_check in signature_types {
            let base_type_str = strip_nullable(strip_array(&type_to_check.get_type())).to_string();
            if let Some(struct_info) = self.struct_table.get_struct(&base_type_str) {
                if !struct_info.is_exported {
                    diagnostics.report_error(
                        format!("Exported function '{}' exposes unexported struct '{}'", function.name.text, base_type_str),
                        Some(function.name.position.clone()),
                    );
                }
            }
        }
    }

    /// Pass 2: analyze the body of every concrete function.
    fn analyze_function_bodies(&mut self, node: &'a ProgramNode<'a>, symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                continue;
            }
            diagnostics.file_path = file_path_string(&function.file_path);
            let table = self.analyze_function(function, diagnostics)?;
            symbol_table_map.insert(function.name.text.clone(), table);
        }
        Ok(())
    }

    /// Pass 3: analyze each monomorphized generic instance so concrete-type errors surface.
    fn analyze_instantiated_generics(&mut self, symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let generics_to_analyze: Vec<(String, &'a FunctionNode<'a>)> = self.instantiated_generics.iter()
            .map(|(mangled, (_bindings, template))| (mangled.clone(), *template))
            .collect();
        for (mangled_name, template) in generics_to_analyze {
            diagnostics.file_path = file_path_string(&template.file_path);
            let table = self.analyze_function(template, diagnostics)?;
            symbol_table_map.insert(mangled_name, table);
        }
        Ok(())
    }

    /// Pass 4: analyze the body of every (de-sugared) struct method.
    fn analyze_struct_method_bodies(&mut self, symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let methods_to_analyze = self.struct_methods.clone();
        for method in methods_to_analyze {
            diagnostics.file_path = file_path_string(&method.file_path);
            let table = self.analyze_function(method, diagnostics)?;
            symbol_table_map.insert(method.name.text.clone(), table);
        }
        Ok(())
    }
    fn ensure_struct_instantiated(&mut self, base_name: &str, args: &[Type], position: &TextSpan, diagnostics: &mut DiagnosticBag) {
        let mangled_name = mangle_generic(base_name, args);
        if self.struct_table.get_struct(&mangled_name).is_some() {
            return;
        }

        let template = match self.generic_structs.get(base_name) {
            Some(template) => *template,
            None => return,
        };

        let params = template.generic_parameters.as_deref().unwrap_or(&[]);
        if args.len() != params.len() {
            diagnostics.report_error(
                format!("Generic struct '{}' expects {} type argument(s), but {} were provided", base_name, params.len(), args.len()),
                Some(position.clone()),
            );
        }
        let bindings = generic_bindings(params, args);

        let new_fields = template.fields.iter()
            .map(|field| StructFieldNode {
                name: field.name.clone(),
                type_token: substitute_generic_token(&field.type_token, &bindings),
            })
            .collect();

        let mut new_name_token = template.name.clone();
        new_name_token.text = mangled_name.clone();
        let new_decl = StructDeclarationNode::new(
            new_name_token,
            None,
            new_fields,
            template.methods.clone(),
            template.is_exported,
        );

        if let Err(e) = self.struct_table.add_struct(&new_decl) {
            diagnostics.report_error(e, Some(position.clone()));
        }

        self.register_struct_methods(&new_decl, &mangled_name, &bindings, diagnostics);
    }

    fn register_struct_methods(&mut self, struct_decl: &StructDeclarationNode<'a>, struct_type_str: &str, bindings: &[(String, String)], diagnostics: &mut DiagnosticBag) {
        for method in &struct_decl.methods {
            let mangled_name = format!("{}_{}", struct_type_str, method.name.text);

            let mut new_method = method.clone();
            new_method.name = synthetic_token(TokenKind::IdentifierToken, &mangled_name);

            if !bindings.is_empty() {
                Self::substitute_generic_signature(&mut new_method, bindings);
            }

            new_method.parameters.insert(0, Self::make_this_param(struct_type_str));

            let method_ref = self.arena.alloc(new_method);
            self.struct_methods.push(method_ref);

            if let Err(e) = self.function_table.add_function(mangled_name.clone(), FunctionTableInfo::from(method_ref)) {
                diagnostics.report_error(e.to_string(), Some(method.name.position.clone()));
            }
        }
    }

    /// Substitutes every generic parameter appearing in a method's parameter or return types
    /// with its concrete type, according to the monomorphization bindings.
    fn substitute_generic_signature(method: &mut FunctionNode<'a>, bindings: &[(String, String)]) {
        for param in &mut method.parameters {
            param.type_ = Self::monomorphize_type(&param.type_, bindings);
        }
        if let Some(ret) = &method.return_type {
            method.return_type = Some(Self::monomorphize_type(ret, bindings));
        }
    }

    /// Determines the concrete type bound to each generic parameter of `template` for one call.
    /// Uses explicit type arguments when given (arity-checked); otherwise infers each parameter
    /// from the actual argument passed to the first formal parameter that is exactly that
    /// parameter. Parameters that cannot be inferred produce a diagnostic.
    fn infer_generic_bindings(&self, template: &FunctionNode<'a>, generic_args: &Option<Vec<Type>>, params_types: &[String], position: &TextSpan, diagnostics: &mut DiagnosticBag) -> Vec<(String, String)> {
        let gen_params = template.generic_parameters.as_deref().unwrap_or(&[]);

        if let Some(generics) = generic_args {
            if !generics.is_empty() {
                if generics.len() != gen_params.len() {
                    diagnostics.report_error(
                        format!("Generic function '{}' expects {} type argument(s), but {} were provided", template.name.text, gen_params.len(), generics.len()),
                        Some(position.clone()),
                    );
                }
                return gen_params.iter()
                    .zip(generics.iter())
                    .map(|(param, arg)| (param.text.clone(), arg.get_type()))
                    .collect();
            }
        }

        gen_params.iter().map(|param| {
            let concrete = template.parameters.iter().enumerate().find_map(|(i, formal)| {
                match &formal.type_ {
                    Type::Struct(token, None) if token.text == param.text => params_types.get(i).cloned(),
                    _ => None,
                }
            });
            match concrete {
                Some(concrete) => (param.text.clone(), concrete),
                None => {
                    diagnostics.report_error(
                        format!("Cannot infer generic parameter '{}' of function '{}'; specify type arguments explicitly", param.text, template.name.text),
                        Some(position.clone()),
                    );
                    (param.text.clone(), "void".to_string())
                }
            }
        }).collect()
    }

    /// Returns `ty` with any generic parameter substituted for its concrete type per the
    /// monomorphization bindings, recursing through array and nullable wrappers (`T`, `T[]`, `T?`).
    fn monomorphize_type(ty: &Type, bindings: &[(String, String)]) -> Type {
        match ty {
            Type::Struct(token, None) => match lookup_binding(bindings, &token.text) {
                Some(concrete) => Self::concrete_type_from_str(&concrete),
                None => ty.clone(),
            },
            Type::Array(inner) => Type::Array(Box::new(Self::monomorphize_type(inner, bindings))),
            Type::Nullable(inner) => Type::Nullable(Box::new(Self::monomorphize_type(inner, bindings))),
            _ => ty.clone(),
        }
    }

    /// Builds the implicit `this` parameter injected as the first argument of every method.
    fn make_this_param(struct_type_str: &str) -> ParameterNode {
        let this_type = Type::Struct(synthetic_token(TokenKind::IdentifierToken, struct_type_str), None);
        ParameterNode::new(synthetic_token(TokenKind::IdentifierToken, "this"), this_type)
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

        // Monomorphization: bind every generic parameter to a concrete type, then register
        // (once) a specialized signature under the mangled name.
        if self.generic_functions.contains_key(&function_name) {
            let template = *self.generic_functions.get(&function_name).unwrap();
            let bindings = self.infer_generic_bindings(template, generic_args, &params_types, &name.position, diagnostics);
            let mangled_name = mangle_bindings(&function_name, &bindings);

            if self.function_table.get_function(&mangled_name).is_err() {
                self.instantiated_generics.insert(mangled_name.clone(), (bindings.clone(), template));

                let info = FunctionTableInfo {
                    name: mangled_name.clone(),
                    parameters: template.parameters.iter()
                        .map(|p| Self::monomorphize_type(&p.type_, &bindings).get_type())
                        .collect(),
                    return_type: template.return_type.as_ref()
                        .map(|ret| Self::monomorphize_type(ret, &bindings)),
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
    fn analyze_method_call(&mut self, obj: &ExpressionNode<'a>, method: &SyntaxToken, _generic_args: &Option<Vec<Type>>, params: &Vec<ExpressionNode<'a>>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<Type, ()> {
        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

        let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
            Some(parts) => parts,
            None => {
                diagnostics.report_error(format!("Cannot call method on non-struct type {}", obj_type.get_type()), Some(method.position.clone()));
                return Ok(Type::Void);
            }
        };

        self.ensure_struct_instantiated(&base_name, &generic_args, &method.position, diagnostics);
        let struct_name = mangle_generic(&base_name, &generic_args);
        
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
            diagnostics.report_error(format!("while condition must be bool, got {}", cond_type.get_type()), condition.position());
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
                diagnostics.report_error(format!("for condition must be bool, got {}", cond_type.get_type()), cond_expr.position());
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
        let l = match (*symbol_table).as_ref().borrow().get_symbol(left) {
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
                diagnostics.report_error(format!("Cannot index into non-array type {}", array_type.get_type()), arr.position());
                return Ok(());
            }
        };

        let index_type = self.analyze_expression(index, parent_function, symbol_table, diagnostics)?;
        if index_type.get_type() != "int" {
            diagnostics.report_error(format!("Array index must be of type int, got {}", index_type.get_type()), index.position());
        }

        let right_type = self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        self.compare_data_type(&inner_type, &right_type, &empty_span(), diagnostics)?;
        
        Ok(())
    }

    fn analyze_member_assignment(&mut self, obj: &ExpressionNode<'a>, member: &SyntaxToken, right: &ExpressionNode<'a>, parent_function: &FunctionNode<'a>, symbol_table: &Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag) -> Result<(), ()> {
        let obj_type = self.analyze_expression(obj, parent_function, symbol_table, diagnostics)?;

        let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
            Some(parts) => parts,
            None => {
                diagnostics.report_error(format!("Cannot access member of non-struct type {}", obj_type.get_type()), Some(member.position.clone()));
                return Ok(());
            }
        };

        self.ensure_struct_instantiated(&base_name, &generic_args, &member.position, diagnostics);
        let struct_name = mangle_generic(&base_name, &generic_args);

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
        match expression
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
                    self.compare_data_type(&first_type, &element_type, &empty_span(), diagnostics)?;
                }
                
                Ok(Type::Array(Box::new(first_type)))
            },
            ExpressionNode::IndexAccess(array_expr, index_expr) => {
                let array_type = self.analyze_expression(array_expr, parent_function, symbol_table, diagnostics)?;
                let inner_type = match array_type {
                    Type::Array(inner) => *inner,
                    _ => {
                        diagnostics.report_error(format!("Cannot index into non-array type {}", array_type.get_type()), array_expr.position());
                        Type::Void
                    }
                };
                
                let index_type = self.analyze_expression(index_expr, parent_function, symbol_table, diagnostics)?;
                if index_type.get_type() != "int" {
                    diagnostics.report_error(format!("Array index must be of type int, got {}", index_type.get_type()), index_expr.position());
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
            ExpressionNode::IsExpression(left, _right_type) => {
                // `is` always evaluates to a bool; the actual comparison is resolved at compile time.
                self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                Ok(Type::Boolean(synthetic_token(TokenKind::BooleanToken, "true")))
            },
            ExpressionNode::Parenthesized(expr)=>
                Ok(self.analyze_expression(expr,parent_function,symbol_table, diagnostics)?),
            ExpressionNode::StructInstantiation(name, generic_args, fields) => {
                let generic_args_slice = generic_args.as_deref().unwrap_or(&[]);
                let struct_name = mangle_generic(&name.text, generic_args_slice);

                // Monomorphize generic struct if needed
                self.ensure_struct_instantiated(&name.text, generic_args_slice, &name.position, diagnostics);
                
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

                let (base_name, generic_args) = match Self::resolve_struct_parts(&obj_type) {
                    Some(parts) => parts,
                    None => {
                        diagnostics.report_error(format!("Cannot access member of non-struct type {}", obj_type.get_type()), Some(member.position.clone()));
                        return Ok(Type::Void);
                    }
                };

                self.ensure_struct_instantiated(&base_name, &generic_args, &member.position, diagnostics);
                let struct_name = mangle_generic(&base_name, &generic_args);

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

                // If the target (after peeling array wrappers) is a generic struct, instantiate it.
                let mut core_target = target_type;
                while let Type::Array(inner) = core_target {
                    core_target = inner;
                }
                if let Some((base_name, generic_args)) = Self::resolve_struct_parts(core_target) {
                    self.ensure_struct_instantiated(&base_name, &generic_args, &empty_span(), diagnostics);
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
                    diagnostics.report_error(format!("Cannot cast from {} to {}", expr_type_str, target_type_str), target_type.get_span().or_else(|| expr.position()));
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
        
        // A nullable `T?` accepts another `T?`, a plain `T`, or the `null` literal (`void?`).
        if let Type::Nullable(inner) = left {
            if let Type::Nullable(inner_right) = right {
                if inner.get_type() == inner_right.get_type() {
                    return Ok(());
                }
            } else if inner.get_type() == right.get_type() {
                return Ok(());
            }
            if right.get_type() == "void?" {
                return Ok(());
            }
        }

        // Any reference type (or nullable) can be compared against the `null` literal.
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
        if self.struct_table.is_reference_type(type_name) {
            return true;
        }
        // A not-yet-instantiated generic struct instance (e.g. `Box_int`) is also a reference type.
        let base_name = strip_nullable(type_name);
        self.demangle_generic_struct(base_name).is_some()
    }
    fn analyze_identifier(&mut self,id:&SyntaxToken,symbol_table:&Rc<RefCell<SymbolTable>>, diagnostics: &mut DiagnosticBag)->Result<Type,()> {
        let r= match (*symbol_table).as_ref().borrow().get_symbol(id) {
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
                diagnostics.report_error(format!("if condition must be bool, got {}", cond_type.get_type()), condition.position());
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
                    diagnostics.report_error(format!("else if condition must be bool, got {}", elif_cond_type.get_type()), i.0.position());
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