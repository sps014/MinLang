use std::io::{Error, ErrorKind};
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;

/// Represents a data type in the language
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Integer(SyntaxToken),
    Float(SyntaxToken),
    String(SyntaxToken),
    Boolean(SyntaxToken),
    Void,
}

impl Type {
    /// Returns the string representation of the type
    pub fn get_type(&self) -> String {
        match self {
            Type::Integer(_) => "int",
            Type::Float(_) => "float",
            Type::String(_) => "string",
            Type::Void => "void",
            Type::Boolean(_) => "bool",
        }
        .to_string()
    }

    /// Returns the line and column string of the type token
    pub fn get_line_str(&self) -> String {
        match self {
            Type::Integer(token) => token.position.get_point_str(),
            Type::Float(token) => token.position.get_point_str(),
            Type::String(token) => token.position.get_point_str(),
            Type::Void => "".to_string(),
            Type::Boolean(token) => token.position.get_point_str(),
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
            _ => return Err(Error::new(ErrorKind::Other, "TypeLiteral::from_token: Unexpected token kind")),
        };
        Ok(r)
    }
}
