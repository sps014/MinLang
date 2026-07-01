//! The top-level, typed error returned by [`crate::driver::compiler::Compiler::compile`]. Each
//! variant names the pipeline phase that failed. User-facing detail for `Syntax`/`Semantic` lives
//! in the diagnostics that were already rendered; `Io` wraps lower-level source/artifact failures
//! and `Codegen` wraps a backend failure.

use crate::codegen::CodegenError;
use std::fmt;

#[derive(Debug)]
pub enum CompileError {
    /// One or more syntax errors were reported during parsing/import resolution.
    Syntax,
    /// One or more semantic errors were reported during analysis.
    Semantic,
    /// An I/O failure during the pipeline (reading sources, writing artifacts).
    Io(std::io::Error),
    /// The code generator failed.
    Codegen(CodegenError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Syntax => write!(f, "Syntax errors found during parsing"),
            CompileError::Semantic => write!(f, "Semantic errors found"),
            CompileError::Io(e) => write!(f, "{}", e),
            CompileError::Codegen(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<std::io::Error> for CompileError {
    fn from(e: std::io::Error) -> Self {
        CompileError::Io(e)
    }
}

impl From<CodegenError> for CompileError {
    fn from(e: CodegenError) -> Self {
        CompileError::Codegen(e)
    }
}
