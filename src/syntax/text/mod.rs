//! Back-compat re-export. The span/text/writer leaf types now live in the crate-root
//! [`crate::text`] module so that `diagnostics` can depend on them without `syntax` and
//! `diagnostics` forming a module cycle. Existing `syntax::text::*` paths keep working.
pub use crate::text::{indented_text_writer, line_text, text_span};
