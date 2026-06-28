use std::io::{Error, ErrorKind};
use crate::syntax::token::syntax_token::SyntaxToken;

/// Returns the given type name with a single trailing nullable (`?`) suffix removed.
pub fn strip_nullable(type_name: &str) -> &str {
    type_name.strip_suffix('?').unwrap_or(type_name)
}

/// Single source of truth for name mangling: joins `base` with each suffix using `_`
/// separators, e.g. base `Pair` with `["int", "string"]` becomes `Pair_int_string`.
/// With no suffixes the base name is returned unchanged.
pub fn mangle_with_suffixes<S: AsRef<str>>(base: &str, suffixes: impl IntoIterator<Item = S>) -> String {
    let mut name = base.to_string();
    for suffix in suffixes {
        name.push('_');
        name.push_str(suffix.as_ref());
    }
    name
}

/// Builds the monomorphized name for a generic instantiation by appending every concrete
/// type argument, e.g. base `Pair` with `[int, string]` becomes `Pair_int_string`. With no
/// arguments the base name is returned unchanged.
pub fn mangle_generic(base: &str, args: &[Type]) -> String {
    mangle_with_suffixes(base, args.iter().map(|arg| arg.get_type()))
}

/// Maps a C#/.NET-style type name to its canonical Dream primitive spelling, or returns
/// `None` if `name` is not a recognized alias. `String`/`Int32`/... become
/// `string`/`int`/..., so the two spellings are fully interchangeable while every
/// downstream stage continues to see the lowercase canonical names.
pub fn canonical_type_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "String" => "string",
        "Int32" => "int",
        "Int64" => "int",
        "Single" => "float",
        "Double" => "double",
        "Boolean" => "bool",
        "Char" => "char",
        "Object" => "object",
        "Void" => "void",
        _ => return None,
    })
}

/// Constructs the primitive `Type` named by `name`, backed by `token`, or returns `None`
/// if `name` does not denote a primitive. Single source of truth for primitive construction.
pub fn primitive_type(name: &str, token: SyntaxToken) -> Option<Type> {
    Some(match name {
        "int" => Type::Integer(token),
        "float" => Type::Float(token),
        "double" => Type::Double(token),
        "string" => Type::String(token),
        "bool" => Type::Boolean(token),
        "char" => Type::Char(token),
        _ => return None,
    })
}

/// Returns the given type name with a single trailing array (`[]`) suffix removed.
pub fn strip_array(type_name: &str) -> &str {
    type_name.strip_suffix("[]").unwrap_or(type_name)
}

/// The canonical type-name string for a `Future<T>` whose inner type is `inner`
/// (e.g. `Future_int`). `Future` is the storable, ref-light handle returned by async calls.
pub fn future_type_name(inner: &str) -> String {
    format!("Future_{}", inner)
}

/// If `type_name` denotes a `Future<T>` (i.e. `Future_<inner>`), returns the inner type name,
/// otherwise `None`. Used to type `await` (unwrap) and async-call results (wrap).
pub fn future_inner(type_name: &str) -> Option<&str> {
    type_name.strip_prefix("Future_")
}

/// Maps a type name to the suffix used in its generated `$release_*` runtime helper.
/// Arrays become `_array` and nullable markers are dropped (e.g. `Node[]?` -> `Node_array`).
pub fn release_func_suffix(type_name: &str) -> String {
    type_name.replace("[]", "_array").replace('?', "")
}

/// Returns true if a type name denotes a heap-allocated, reference-counted value
/// (strings, arrays, and structs). `known_struct` decides whether a bare name is a struct.
pub fn is_reference_type_name(type_name: &str, known_struct: impl Fn(&str) -> bool) -> bool {
    let base = strip_nullable(type_name);
    base == "string" || base == "object" || base.ends_with("[]") || known_struct(base)
}

/// Represents a data type in the language
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Integer(SyntaxToken),
    Float(SyntaxToken),
    Double(SyntaxToken),
    String(SyntaxToken),
    Boolean(SyntaxToken),
    /// A single character. Stored as an `i32` code point on the stack but only one byte in
    /// memory (arrays/fields use `i32.load8_u`/`i32.store8`). A value type (not ref-counted).
    Char(SyntaxToken),
    /// The universal top type. At runtime an `object` is an `i32` pointer to a tagged heap
    /// block: primitives are boxed, reference types are stored directly (their block carries
    /// the tag in its header).
    Object(SyntaxToken),
    Array(Box<Type>),
    Struct(SyntaxToken, Option<Vec<Type>>),
    Generic(String),
    Nullable(Box<Type>),
    /// A first-class function value `fun(params...): ret`. Represented at runtime as an `i32`
    /// index into the module's function table (used with `call_indirect`).
    Function(Vec<Type>, Box<Type>),
    Void,
}

impl Type {
    /// Returns the string representation of the type
    pub fn get_type(&self) -> String {
        match self {
            Type::Integer(_) => "int".to_string(),
            Type::Float(_) => "float".to_string(),
            Type::Double(_) => "double".to_string(),
            Type::String(_) => "string".to_string(),
            Type::Object(_) => "object".to_string(),
            Type::Void => "void".to_string(),
            Type::Boolean(_) => "bool".to_string(),
            Type::Char(_) => "char".to_string(),
            Type::Array(inner) => format!("{}[]", inner.get_type()),
            Type::Struct(token, generic_args) => {
                match generic_args {
                    Some(args) => mangle_generic(&token.text, args),
                    None => token.text.clone(),
                }
            },
            Type::Generic(name) => name.clone(),
            Type::Nullable(inner) => format!("{}?", inner.get_type()),
            Type::Function(params, ret) => {
                let params_str = params.iter().map(|p| p.get_type()).collect::<Vec<_>>().join(",");
                format!("fun({}):{}", params_str, ret.get_type())
            }
        }
    }

    /// Returns true if this type is a nullable (`T?`) type.
    pub fn is_nullable(&self) -> bool {
        matches!(self, Type::Nullable(_))
    }

    /// Returns true if this type is an array (`T[]`) type.
    pub fn is_array(&self) -> bool {
        matches!(self, Type::Array(_))
    }

    /// Returns the type name with any trailing nullable (`?`) suffix removed.
    pub fn base_name(&self) -> String {
        strip_nullable(&self.get_type()).to_string()
    }

    /// Returns the source span of the token backing this type, if any.
    /// Composite types (arrays, nullables) defer to their inner type; `Void`/`Generic`
    /// have no backing token and return `None`.
    pub fn get_span(&self) -> Option<crate::syntax::text::text_span::TextSpan> {
        match self {
            Type::Integer(token)
            | Type::Float(token)
            | Type::Double(token)
            | Type::String(token)
            | Type::Boolean(token)
            | Type::Char(token)
            | Type::Object(token)
            | Type::Struct(token, _) => Some(token.position.clone()),
            Type::Array(inner) | Type::Nullable(inner) => inner.get_span(),
            Type::Void | Type::Generic(_) | Type::Function(_, _) => None,
        }
    }

    /// Returns the line and column string of the type token
    pub fn get_line_str(&self) -> String {
        match self {
            Type::Integer(token) => token.position.get_point_str(),
            Type::Float(token) => token.position.get_point_str(),
            Type::Double(token) => token.position.get_point_str(),
            Type::String(token) => token.position.get_point_str(),
            Type::Object(token) => token.position.get_point_str(),
            Type::Void => "".to_string(),
            Type::Boolean(token) => token.position.get_point_str(),
            Type::Char(token) => token.position.get_point_str(),
            Type::Array(inner) => inner.get_line_str(),
            Type::Struct(token, _) => token.position.get_point_str(),
            Type::Generic(_) => "".to_string(), // Can be improved
            Type::Nullable(inner) => inner.get_line_str(),
            Type::Function(_, _) => "".to_string(),
        }
    }

    /// Parses a Type from a given SyntaxToken
    pub fn from_token(mut token: SyntaxToken) -> Result<Type, Error> {
        // Normalize C#/.NET-style type names (String, Int32, ...) to their canonical Dream
        // primitive spelling before any further classification, so the two are interchangeable.
        if let Some(canonical) = canonical_type_name(&token.text) {
            token.text = canonical.to_string();
        }
        if let Some(primitive) = primitive_type(&token.text, token.clone()) {
            return Ok(primitive);
        }
        let r = match token.text.as_str() {
            "object" => Type::Object(token),
            "void" => Type::Void,
            _ => {
                if token.text.ends_with("?") {
                    let base_type_str = &token.text[0..token.text.len() - 1];
                    let mut base_token = token.clone();
                    base_token.text = base_type_str.to_string();
                    let base_type = Type::from_token(base_token)?;
                    
                    // Restrict nullable to reference types
                    match &base_type {
                        Type::String(_) | Type::Object(_) | Type::Array(_) | Type::Struct(_, _) | Type::Void => {
                            return Ok(Type::Nullable(Box::new(base_type)));
                        },
                        _ => {
                            return Err(Error::new(ErrorKind::Other, format!("Type '{}' cannot be nullable. Only reference types (string, arrays, structs) can be nullable.", base_type.get_type())));
                        }
                    }
                }
                // Handle array types like "int[]" or "Point[]"
                if token.text.ends_with("[]") {
                    let base_type_str = &token.text[0..token.text.len() - 2];
                    let mut base_token = token.clone();
                    base_token.text = base_type_str.to_string();
                    let base_type = Type::from_token(base_token)?;
                    return Ok(Type::Array(Box::new(base_type)));
                }
                // If it's not a built-in type or array, assume it's a struct type
                return Ok(Type::Struct(token, None));
            }
        };
        Ok(r)
    }
}
