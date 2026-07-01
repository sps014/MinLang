//! The Dream front-end: lexer, AST node definitions, parser, and the syntax tree. Depends only on
//! `dream-text` (source primitives) and `dream-diagnostics` (error reporting), so it forms the
//! middle layer of the front-end crate stack and never reaches back into semantics or codegen.
pub mod lexer;
pub mod nodes;
pub mod parser;
pub mod precedence;
pub mod syntax_tree;
pub mod token;
