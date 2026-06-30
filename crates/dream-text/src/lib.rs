//! Leaf crate holding the source-text primitives shared by the front-end: text spans, line-width
//! bookkeeping, and the indented WAT writer. Depends on nothing else in the workspace so both
//! `dream-diagnostics` and `dream-syntax` can build on it without forming a cycle.
pub mod indented_text_writer;
pub mod line_text;
pub mod text_span;
