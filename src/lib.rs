pub mod codegen;
pub mod driver;
#[cfg(feature = "native")]
pub mod execution;
pub mod intrinsics;
pub mod semantics;
pub mod stdlib;

// The front-end now lives in three layered crates. Re-export them under their historical module
// names so every `crate::syntax::...`, `crate::diagnostics::...`, and `crate::text::...` path (and
// the LSP's `dream::syntax::...` / `dream::diagnostics`) keeps resolving unchanged.
pub use dream_diagnostics as diagnostics;
pub use dream_syntax as syntax;
pub use dream_text as text;
