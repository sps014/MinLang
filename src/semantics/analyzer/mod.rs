use crate::driver::diagnostics::DiagnosticBag;
use crate::semantics::function_table::FunctionTable;
use crate::semantics::struct_table::StructTable;
use crate::semantics::symbol_table::SymbolTable;
use crate::semantics::union_table::UnionTable;
use crate::syntax::nodes::types::{mangle_with_suffixes, primitive_type, FUTURE_TYPE};
use crate::syntax::nodes::{EnumDeclarationNode, ExtendNode};
use crate::syntax::nodes::{FunctionNode, ProgramNode, Type};
use crate::syntax::syntax_tree::SyntaxTree;
use crate::syntax::text::line_text::LineText;
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use bumpalo::Bump;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod await_rules;
mod calls;
mod declarations;
mod expressions;
mod generics;
mod match_unions;
mod statements;
mod type_checker;

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
    params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (param.text.clone(), arg.get_type()))
        .collect()
}

/// Looks up the concrete type bound to a generic parameter name, if any.
fn lookup_binding(bindings: &[(String, String)], name: &str) -> Option<String> {
    bindings
        .iter()
        .find(|(param, _)| param == name)
        .map(|(_, concrete)| concrete.clone())
}

/// Builds a mangled function name by appending each concrete type from the bindings in order,
/// e.g. base `swap` with bindings `[(T,int),(V,string)]` becomes `swap_int_string`.
fn mangle_bindings(base: &str, bindings: &[(String, String)]) -> String {
    mangle_with_suffixes(base, bindings.iter().map(|(_, concrete)| concrete.as_str()))
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

/// Rewrites a structured field type, substituting any generic parameter that appears in it with
/// its bound concrete type. Unlike `substitute_generic_token` (which only understands `T`, `T[]`,
/// `T?` on a flat token), this recurses through arrays, nullables, generic arguments, and function
/// types, so a field like `List<T>` becomes `List<JsonValue>` rather than being flattened.
fn substitute_generic_type(ty: &Type, bindings: &[(String, String)]) -> Type {
    match ty {
        Type::Array(inner) => Type::Array(Box::new(substitute_generic_type(inner, bindings))),
        Type::Nullable(inner) => Type::Nullable(Box::new(substitute_generic_type(inner, bindings))),
        Type::Function(params, ret) => Type::Function(
            params
                .iter()
                .map(|p| substitute_generic_type(p, bindings))
                .collect(),
            Box::new(substitute_generic_type(ret, bindings)),
        ),
        Type::Generic(name) => bind_concrete(name, bindings).unwrap_or_else(|| ty.clone()),
        Type::Struct(token, args) => {
            // A bare struct whose name is itself a generic parameter (the common `T` case, since
            // unknown identifiers parse as `Type::Struct`).
            if args.is_none() {
                if let Some(concrete) = bind_concrete(&token.text, bindings) {
                    return concrete;
                }
            }
            let new_args = args.as_ref().map(|a| {
                a.iter()
                    .map(|x| substitute_generic_type(x, bindings))
                    .collect()
            });
            Type::Struct(token.clone(), new_args)
        }
        other => other.clone(),
    }
}

/// Resolves a generic parameter name to its bound concrete `Type` (parsing the stored type-name
/// string back into a `Type`), or `None` if `name` is not a bound generic parameter.
fn bind_concrete(name: &str, bindings: &[(String, String)]) -> Option<Type> {
    let concrete = lookup_binding(bindings, name)?;
    Type::from_token(synthetic_token(TokenKind::IdentifierToken, &concrete)).ok()
}

/// Maps each generic parameter name to the concrete type bound to it for one monomorphization.
pub type GenericBindings = Vec<(String, String)>;

/// Enum name -> (member name -> integer value).
pub type EnumTable = HashMap<String, HashMap<String, i32>>;

/// A resolved top-level variable, carried from semantic analysis into code generation so the
/// generator can emit the corresponding WASM global and the module-init store (and decide whether
/// to export it to the host).
#[derive(Debug, Clone)]
pub struct GlobalSymbol {
    pub name: String,
    /// The resolved (non-generic) type name, e.g. `int`, `string`, `Point`.
    pub type_str: String,
    pub is_const: bool,
    pub is_public: bool,
    pub is_static: bool,
}

pub struct SemanticInfo<'a> {
    pub hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>,
    pub function_table: &'a FunctionTable,
    pub struct_table: &'a StructTable,
    pub instantiated_generics: HashMap<String, (GenericBindings, &'a FunctionNode<'a>)>,
    pub struct_methods: Vec<(&'a FunctionNode<'a>, GenericBindings)>,
    pub enums: EnumTable,
    /// Layout of every (monomorphized) discriminated union, surfaced to codegen so it can
    /// allocate variant blocks, lower `match`, and emit discriminant-aware releases.
    pub unions: UnionTable,
    pub globals: Vec<GlobalSymbol>,
}

impl<'a> SemanticInfo<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hash_map: HashMap<String, Rc<RefCell<SymbolTable>>>,
        function_table: &'a FunctionTable,
        struct_table: &'a StructTable,
        instantiated_generics: HashMap<String, (GenericBindings, &'a FunctionNode<'a>)>,
        struct_methods: Vec<(&'a FunctionNode<'a>, GenericBindings)>,
        enums: EnumTable,
        unions: UnionTable,
        globals: Vec<GlobalSymbol>,
    ) -> SemanticInfo<'a> {
        SemanticInfo {
            hash_map,
            function_table,
            struct_table,
            instantiated_generics,
            struct_methods,
            enums,
            unions,
            globals,
        }
    }
}

/// Groups context arguments frequently passed together to simplify function signatures.
pub struct AnalyzerContext<'a, 'b> {
    pub parent_function: &'b FunctionNode<'a>,
    pub symbol_table: &'b Rc<RefCell<SymbolTable>>,
}

pub struct Analyzer<'a> {
    syntax_tree: &'a SyntaxTree<'a>,
    function_table: FunctionTable,
    struct_table: StructTable,
    arena: &'a Bump,
    generic_functions: HashMap<String, &'a FunctionNode<'a>>,
    instantiated_generics: HashMap<String, (GenericBindings, &'a FunctionNode<'a>)>,
    generic_structs:
        HashMap<String, &'a crate::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    struct_methods: Vec<(&'a FunctionNode<'a>, GenericBindings)>,
    /// Registered enums: name -> (member -> value). Enum values are plain `i32`s at runtime.
    enum_table: EnumTable,
    /// Layout of every registered (monomorphized) discriminated union.
    union_table: UnionTable,
    /// Generic discriminated-union templates (`enum Option<T> { ... }`), instantiated on demand.
    generic_unions: HashMap<String, &'a EnumDeclarationNode>,
    /// Generic `extend Type<...> { ... }` templates (e.g. `extend Option<T> { ... }`), keyed by
    /// the extended type's name. Their methods are monomorphized alongside each concrete
    /// instantiation of the target generic union or struct (see `ensure_*_instantiated`).
    generic_extends: HashMap<String, &'a ExtendNode<'a>>,
    /// An optional expected type for the expression currently being analyzed (from a `let`
    /// annotation or `return` type). Used to resolve the type arguments of a generic union's
    /// nullary variant (`let o: Option<int> = Option.None;`), where they cannot be inferred from
    /// arguments. `None` outside such contexts.
    current_expected_type: Option<Type>,
    /// The generic substitution bindings active while analyzing a monomorphized function or
    /// struct-method body. Empty outside of any generic instantiation. Used to resolve generic
    /// type parameters that appear inside a body (e.g. the `T` in `array_new<T>(...)`).
    current_generic_bindings: GenericBindings,
    /// Stack of loop labels currently in scope, so `break label;`/`continue label;` can be
    /// validated against an enclosing labeled loop.
    loop_labels: Vec<String>,
    /// True while analyzing the body of an `async fun`. Gates the use of `await`.
    current_function_is_async: bool,
    /// Resolved top-level variables, in declaration order. Surfaced to codegen via [`SemanticInfo`].
    globals: Vec<GlobalSymbol>,
    /// The module-level symbol scope holding every top-level variable. It is the root parent of
    /// every function's parameter table, so function bodies resolve global identifiers (and their
    /// `const`-ness) through ordinary lexical lookup.
    global_symbol_table: Rc<RefCell<SymbolTable>>,
}
impl<'a> Analyzer<'a> {
    pub fn new(tree: &'a SyntaxTree<'a>, arena: &'a Bump) -> Self {
        Self {
            syntax_tree: tree,
            function_table: FunctionTable::new(),
            struct_table: StructTable::new(),
            arena,
            generic_functions: HashMap::new(),
            instantiated_generics: HashMap::new(),
            generic_structs: HashMap::new(),
            struct_methods: Vec::new(),
            enum_table: HashMap::new(),
            union_table: HashMap::new(),
            generic_unions: HashMap::new(),
            generic_extends: HashMap::new(),
            current_expected_type: None,
            current_generic_bindings: Vec::new(),
            loop_labels: Vec::new(),
            current_function_is_async: false,
            globals: Vec::new(),
            global_symbol_table: Rc::new(RefCell::new(SymbolTable::new(None))),
        }
    }

    /// Builds the `Future<T>` type carrying inner type `inner`. Async-call results are this type,
    /// and `await` unwraps it back to `inner`.
    pub(super) fn future_type(inner: Type) -> Type {
        Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, FUTURE_TYPE),
            Some(vec![inner]),
        )
    }

    /// If `ty` is a `Future<T>`, returns the inner `T`; otherwise `None`.
    pub(super) fn future_inner_type(ty: &Type) -> Option<Type> {
        match ty {
            Type::Struct(token, Some(args)) if token.text == FUTURE_TYPE && args.len() == 1 => {
                Some(args[0].clone())
            }
            _ => None,
        }
    }
    #[allow(clippy::result_unit_err)]
    pub fn analyze(&mut self, diagnostics: &mut DiagnosticBag) -> Result<SemanticInfo<'_>, ()> {
        let pgm = self.syntax_tree.get_root();
        self.analyze_pgm(pgm, diagnostics)
    }

    /// Builds a concrete `Type` from a type name, used when substituting a generic
    /// parameter `T` with the concrete type chosen at the call/instantiation site.
    fn concrete_type_from_str(name: &str) -> Type {
        let token = synthetic_token(TokenKind::DataTypeToken, name);
        primitive_type(name, token.clone()).unwrap_or(Type::Struct(token, None))
    }

    /// If `ty` is a struct (or nullable struct), returns its base name and the list of
    /// concrete generic type arguments (empty for non-generic structs). Returns `None`
    /// for any non-struct type. Does NOT recurse into arrays (a method/member access on an
    /// array is invalid and must surface as an error).
    fn resolve_struct_parts(ty: &Type) -> Option<(String, Vec<Type>)> {
        match ty {
            Type::Struct(token, args) => {
                Some((token.text.clone(), args.clone().unwrap_or_default()))
            }
            Type::Nullable(inner) => Self::resolve_struct_parts(inner),
            _ => None,
        }
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
    fn analyze_pgm(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<SemanticInfo<'_>, ()> {
        let mut symbol_table_map = HashMap::new();

        // Stash generic `extend` templates before any type instantiation can occur (a concrete
        // union/struct field may instantiate a generic union during `register_enums`), so the
        // extension methods are always available to attach at the first instantiation.
        self.stash_generic_extensions(node);
        self.register_enums(node, diagnostics);
        self.register_structs(node, diagnostics);
        self.register_extensions(node, diagnostics);
        self.register_functions(node, diagnostics);
        // Globals are analyzed after functions/types are known (so initializers can call them) but
        // before function bodies, so those bodies can resolve global identifiers.
        self.register_globals(node, diagnostics);
        self.analyze_function_bodies(node, &mut symbol_table_map, diagnostics)?;
        self.analyze_pending_instantiations(&mut symbol_table_map, diagnostics)?;

        Ok(SemanticInfo::new(
            symbol_table_map,
            &self.function_table,
            &self.struct_table,
            self.instantiated_generics.clone(),
            self.struct_methods.clone(),
            self.enum_table.clone(),
            self.union_table.clone(),
            self.globals.clone(),
        ))
    }
}

#[cfg(test)]
#[path = "../tests/analyzer_tests.rs"]
mod tests;
