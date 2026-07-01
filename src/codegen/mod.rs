use std::fmt;

pub mod wasm;

/// Errors produced by a code generator. Variants classify by cause so the driver can distinguish
/// user-actionable problems (`Unsupported`) from backend invariant violations (`Internal`) that
/// the analyzer was expected to rule out.
#[derive(Debug)]
pub enum CodegenError {
    /// A name the analyzer should have resolved (variable, operator, symbol) was missing.
    UnknownSymbol(String),
    /// A type name with no WASM lowering.
    UnknownType(String),
    /// A referenced definition (class, union, enum, variant, field, member) does not exist.
    UnknownDef(String),
    /// A well-formed construct the WASM backend does not support.
    Unsupported(String),
    /// A backend invariant the analyzer was expected to guarantee was violated (an ICE).
    Internal(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::UnknownSymbol(m)
            | CodegenError::UnknownType(m)
            | CodegenError::UnknownDef(m)
            | CodegenError::Unsupported(m) => write!(f, "{}", m),
            CodegenError::Internal(m) => write!(f, "internal codegen error: {}", m),
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<crate::semantics::errors::SymbolError> for CodegenError {
    fn from(err: crate::semantics::errors::SymbolError) -> Self {
        // A symbol the backend looks up should always exist by the time codegen runs.
        CodegenError::Internal(err.message)
    }
}

pub trait CodeGenerator<'a> {
    fn generate(&mut self) -> Result<String, CodegenError>;
}
