//! Serializable data-transfer objects returned across the WASM boundary as JSON. They use
//! the field names the web layer expects (close to LSP/Monaco shapes) so the TypeScript glue
//! stays thin.

use serde::Serialize;

use crate::position::Range;

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticOut {
    pub range: Range,
    /// `"error"` or `"warning"`.
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenOut {
    pub range: Range,
    /// Semantic classification, e.g. `keyword`, `type`, `string`, `number`, `comment`.
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct HoverOut {
    /// Markdown shown in the hover popup.
    pub contents: String,
    pub range: Range,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionOut {
    pub label: String,
    /// One of the LSP completion kinds in lower case (`keyword`, `function`, `variable`,
    /// `struct`, `field`, `method`, `enum`, `enumMember`, `type`).
    pub kind: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocationOut {
    pub range: Range,
}
