use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

/// Represents a data type in the language
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Integer(SyntaxToken),
    Float(SyntaxToken),
    String(SyntaxToken),
    Boolean(SyntaxToken),
    Array(Box<Type>),
    Void,
}

impl Type {
    /// Returns the string representation of the type
    pub fn get_type(&self) -> String {
        match self {
            Type::Integer(_) => "int".to_string(),
            Type::Float(_) => "float".to_string(),
            Type::String(_) => "string".to_string(),
            Type::Void => "void".to_string(),
            Type::Boolean(_) => "bool".to_string(),
            Type::Array(inner) => format!("{}[]", inner.get_type()),
        }
    }

    /// Returns the line and column string of the type token
    pub fn get_line_str(&self) -> String {
        match self {
            Type::Integer(token) => token.position.get_point_str(),
            Type::Float(token) => token.position.get_point_str(),
            Type::String(token) => token.position.get_point_str(),
            Type::Void => "".to_string(),
            Type::Boolean(token) => token.position.get_point_str(),
            Type::Array(inner) => inner.get_line_str(),
        }
    }

    /// Parses a Type from a given SyntaxToken
    pub fn from_token(token: SyntaxToken) -> Result<Type, Error> {
        let r = match token.text.as_str() {
            "int" => Type::Integer(token),
            "float" => Type::Float(token),
            "string" => Type::String(token),
            "void" => Type::Void,
            "bool" => Type::Boolean(token),
            _ => {
                // Handle array types like "int[]"
                if token.text.ends_with("[]") {
                    let base_type_str = &token.text[0..token.text.len() - 2];
                    let mut base_token = token.clone();
                    base_token.text = base_type_str.to_string();
                    let base_type = Type::from_token(base_token)?;
                    return Ok(Type::Array(Box::new(base_type)));
                }
                return Err(Error::new(ErrorKind::Other, format!("TypeLiteral::from_token: Unexpected token kind '{}'", token.text)));
            }
        };
        Ok(r)
    }
}
