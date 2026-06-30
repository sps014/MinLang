//! The Dream front-end: lexer, AST node definitions, parser, and the syntax tree. Depends only on
//! `dream-text` (source primitives) and `dream-diagnostics` (error reporting), so it forms the
//! middle layer of the front-end crate stack and never reaches back into semantics or codegen.
pub mod lexer;
pub mod nodes;
pub mod parser;
pub mod precedence;
pub mod syntax_tree;
pub mod token;

/// Back-compat re-export of the source-text primitives. Existing `syntax::text::*` paths (used by
/// the semantics and codegen layers via the main crate's `syntax` re-export) keep resolving here.
pub mod text {
    pub use dream_text::{indented_text_writer, line_text, text_span};
}
