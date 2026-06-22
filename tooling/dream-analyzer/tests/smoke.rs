//! Native smoke tests for the language-service exports. The `#[wasm_bindgen]` functions are
//! ordinary Rust functions on non-wasm targets, so they can be exercised directly here.

use dream_analyzer::{completions, definition, diagnostics, format_document, hover, references};

const VALID: &str = "fun add(a: int, b: int): int {\nreturn a + b;\n}\nfun main(): void {\nlet x: int = add(1, 2);\nprintln(x);\n}\n";

#[test]
fn valid_program_has_no_errors() {
    let json = diagnostics(VALID);
    assert!(!json.contains("\"severity\":\"error\""), "unexpected errors: {}", json);
}

#[test]
fn invalid_program_reports_error() {
    // `nope` is an undeclared variable, which the analyzer should flag.
    let src = "fun main(): void {\nlet y: int = nope + 1;\n}\n";
    let json = diagnostics(src);
    assert!(json.contains("\"severity\":\"error\""), "expected an error, got: {}", json);
}

#[test]
fn hover_on_function_shows_signature() {
    // Cursor on the `add` call inside main (line 4, char 13 -> the 'd' of add).
    let json = hover(VALID, 4, 14);
    assert!(json.contains("fun add"), "expected signature in hover, got: {}", json);
}

#[test]
fn definition_resolves_call_to_declaration() {
    let json = definition(VALID, 4, 14);
    assert!(json.contains("\"range\""), "expected a definition range, got: {}", json);
    // The declaration is on line 0.
    assert!(json.contains("\"line\":0"), "expected definition on line 0, got: {}", json);
}

#[test]
fn references_include_declaration_and_use() {
    // Cursor on the `add` declaration name.
    let json = references(VALID, 0, 4);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(value.as_array().map(|a| a.len() >= 2).unwrap_or(false), "expected >=2 references, got: {}", json);
}

#[test]
fn completions_include_keywords_and_symbols() {
    let json = completions(VALID, 5, 0);
    assert!(json.contains("\"label\":\"add\""), "expected `add` completion, got: {}", json);
    assert!(json.contains("\"label\":\"if\""), "expected `if` keyword completion, got: {}", json);
}

#[test]
fn formatter_reindents_by_brace_depth() {
    let messy = "fun main(): void {\nlet x: int = 1;\nif (x > 0) {\nprintln(x);\n}\n}\n";
    let formatted = format_document(messy);
    assert!(formatted.contains("\n    let x: int = 1;"), "expected 4-space indent, got:\n{}", formatted);
    assert!(formatted.contains("\n        println(x);"), "expected nested indent, got:\n{}", formatted);
}
