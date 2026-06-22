//! WebAssembly language service for the Dream language. Exposes the compiler front-end
//! (lexer/parser/analyzer) plus a lightweight navigation index to the browser as a set of
//! JSON-returning functions, which the web layer binds to Monaco's provider APIs.

mod analysis;
mod format;
mod index;
mod model;
mod position;
mod tokens;

use wasm_bindgen::prelude::*;

use crate::index::Index;
use crate::model::{CompletionOut, HoverOut, LocationOut};
use crate::position::LineIndex;

/// Installs a panic hook that forwards Rust panics to the browser console. Runs automatically
/// when the module is instantiated.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Returns the diagnostics for `text` as a JSON array of `{ range, severity, message }`.
#[wasm_bindgen]
pub fn diagnostics(text: &str) -> String {
    let diags = analysis::collect_diagnostics(text);
    serde_json::to_string(&diags).unwrap_or_else(|_| "[]".to_string())
}

/// Returns semantic tokens for `text` as a JSON array of `{ range, kind }`.
#[wasm_bindgen]
pub fn semantic_tokens(text: &str) -> String {
    let toks = tokens::classify(text);
    serde_json::to_string(&toks).unwrap_or_else(|_| "[]".to_string())
}

/// Returns the ordered semantic-token legend as a JSON array of category names.
#[wasm_bindgen]
pub fn token_legend() -> String {
    serde_json::to_string(&tokens::TOKEN_LEGEND).unwrap_or_else(|_| "[]".to_string())
}

/// Hover information at the given 0-based line/character. Returns JSON `{ contents, range }`
/// or `null`.
#[wasm_bindgen]
pub fn hover(text: &str, line: u32, character: u32) -> String {
    let line_index = LineIndex::new(text);
    let offset = line_index.offset(line, character);
    let index = Index::build(text);
    let result = index.hover(offset).map(|located| HoverOut {
        contents: located.contents,
        range: line_index.range(located.start, located.end),
    });
    serde_json::to_string(&result).unwrap_or_else(|_| "null".to_string())
}

/// Definition location at the given position. Returns JSON `{ range }` or `null`.
#[wasm_bindgen]
pub fn definition(text: &str, line: u32, character: u32) -> String {
    let line_index = LineIndex::new(text);
    let offset = line_index.offset(line, character);
    let index = Index::build(text);
    let result = index
        .definition(offset)
        .map(|(start, end)| LocationOut { range: line_index.range(start, end) });
    serde_json::to_string(&result).unwrap_or_else(|_| "null".to_string())
}

/// All references (including the declaration) to the symbol at the given position. Returns a
/// JSON array of `{ range }`.
#[wasm_bindgen]
pub fn references(text: &str, line: u32, character: u32) -> String {
    let line_index = LineIndex::new(text);
    let offset = line_index.offset(line, character);
    let index = Index::build(text);
    let locations: Vec<LocationOut> = index
        .references(offset)
        .into_iter()
        .map(|(start, end)| LocationOut { range: line_index.range(start, end) })
        .collect();
    serde_json::to_string(&locations).unwrap_or_else(|_| "[]".to_string())
}

/// Completion proposals at the given position. Returns a JSON array of
/// `{ label, kind, detail }`.
#[wasm_bindgen]
pub fn completions(text: &str, line: u32, character: u32) -> String {
    let line_index = LineIndex::new(text);
    let offset = line_index.offset(line, character);
    let index = Index::build(text);
    let items: Vec<CompletionOut> = index
        .completions(text, offset)
        .into_iter()
        .map(|(label, kind, detail)| CompletionOut { label, kind: kind.completion_kind(), detail })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

/// Returns the reindented document.
#[wasm_bindgen]
pub fn format_document(text: &str) -> String {
    format::format(text)
}
