pub mod abi;
pub mod compiler;
pub mod error;
pub mod json_derive;
pub mod prelude;
pub mod source_loader;
pub mod source_manager;

/// Back-compat re-export. `DiagnosticBag` and friends now live in [`crate::diagnostics`]
/// (a crate-root leaf module) so the front-end (`syntax`) can report diagnostics without
/// `syntax` and `diagnostics` forming a module cycle. Existing `driver::diagnostics::*`
/// paths keep working.
pub use crate::diagnostics;
