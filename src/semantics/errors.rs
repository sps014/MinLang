use std::fmt;

/// Error produced by symbol- and function-table lookups/insertions.
///
/// Previously these tables fabricated `std::io::Error` values for what are
/// really *semantic* failures (e.g. "variable does not exist"). This dedicated
/// type makes the intent explicit while still converting into `std::io::Error`
/// so existing codegen call sites that propagate with `?` keep working and the
/// surfaced message stays identical.
#[derive(Debug, Clone)]
pub struct SymbolError {
    pub message: String,
}

impl SymbolError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for SymbolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SymbolError {}

impl From<SymbolError> for std::io::Error {
    fn from(err: SymbolError) -> Self {
        std::io::Error::other(err.message)
    }
}
