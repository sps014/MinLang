//! Native smoke tests for the language-service exports. The `#[wasm_bindgen]` functions are
//! ordinary Rust functions on non-wasm targets, so they can be exercised directly here.

use dream_analyzer::{completions, definition, diagnostics, format_document, hover, references};

const VALID: &str = "fun add(a: int, b: int): int {\nreturn a + b;\n}\nfun main(): void {\nlet x: int = add(1, 2);\nprintln(x);\n}\n";

#[test]
fn valid_program_has_no_errors() {
    let json = diagnostics(VALID);
    assert!(
        !json.contains("\"severity\":\"error\""),
        "unexpected errors: {}",
        json
    );
}

#[test]
fn invalid_program_reports_error() {
    // `nope` is an undeclared variable, which the analyzer should flag.
    let src = "fun main(): void {\nlet y: int = nope + 1;\n}\n";
    let json = diagnostics(src);
    assert!(
        json.contains("\"severity\":\"error\""),
        "expected an error, got: {}",
        json
    );
}

#[test]
fn hover_on_function_shows_signature() {
    // Cursor on the `add` call inside main (line 4, char 13 -> the 'd' of add).
    let json = hover(VALID, 4, 14);
    assert!(
        json.contains("fun add"),
        "expected signature in hover, got: {}",
        json
    );
}

#[test]
fn definition_resolves_call_to_declaration() {
    let json = definition(VALID, 4, 14);
    assert!(
        json.contains("\"range\""),
        "expected a definition range, got: {}",
        json
    );
    // The declaration is on line 0.
    assert!(
        json.contains("\"line\":0"),
        "expected definition on line 0, got: {}",
        json
    );
}

#[test]
fn references_include_declaration_and_use() {
    // Cursor on the `add` declaration name.
    let json = references(VALID, 0, 4);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        value.as_array().map(|a| a.len() >= 2).unwrap_or(false),
        "expected >=2 references, got: {}",
        json
    );
}

#[test]
fn completions_include_keywords_and_symbols() {
    let json = completions(VALID, 5, 0);
    assert!(
        json.contains("\"label\":\"add\""),
        "expected `add` completion, got: {}",
        json
    );
    assert!(
        json.contains("\"label\":\"if\""),
        "expected `if` keyword completion, got: {}",
        json
    );
}

const PLAYGROUND_SAMPLE: &str = "class Point {\n    x: int;\n    y: int;\n\n    fun magnitude_squared(): int {\n        return this.x * this.x + this.y * this.y;\n    }\n}\n\nenum Color {\n    Red,\n    Green,\n    Blue\n}\n\nfun add(a: int, b: int): int {\n    return a + b;\n}\n\nfun main(): void {\n    let origin: Point = Point { x: 0, y: 0 };\n    let total: int = add(3, 4);\n    println(total);\n}\n";

#[test]
fn completions_work_on_full_sample() {
    // Inside main's body (line 21 is empty-ish near the body), ask for completions.
    let json = completions(PLAYGROUND_SAMPLE, 21, 4);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let len = value.as_array().map(|a| a.len()).unwrap_or(0);
    assert!(
        len > 0,
        "expected completions on the playground sample, got: {}",
        json
    );
    assert!(
        json.contains("\"label\":\"add\""),
        "expected `add`, got: {}",
        json
    );
    assert!(
        json.contains("\"label\":\"origin\""),
        "expected local `origin`, got: {}",
        json
    );
}

/// Converts an ASCII byte offset to a (0-based line, 0-based column) pair for test positioning.
fn line_char(src: &str, byte: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in src.char_indices() {
        if i == byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[test]
fn member_completions_after_dot() {
    // `origin.` -> expect Point members. Build a doc that ends with `origin.`.
    let src = "class Point {\n    x: int;\n    fun mag(): int { return this.x; }\n}\nfun main(): void {\n    let origin: Point = Point { x: 0 };\n    origin.\n}\n";
    // Cursor right after `origin.` on line 6 (0-based), char 11.
    let json = completions(src, 6, 11);
    assert!(
        json.contains("\"label\":\"x\""),
        "expected field `x` after dot, got: {}",
        json
    );
    // Strict member access: enums must NOT leak into member completions.
    assert!(
        !json.contains("\"kind\":\"enum\""),
        "enums should not appear after `.`, got: {}",
        json
    );
}

#[test]
fn this_member_completions_resolve_to_owner() {
    let src = "class Point {\n    x: int;\n    y: int;\n    fun show(): int { let a: int = this.x; return a; }\n}\n";
    // Position the cursor right after `this.`, before `x`.
    let dot = src.find("this.").unwrap() + "this.".len();
    let (line, col) = line_char(src, dot);
    let json = completions(src, line, col);
    assert!(
        json.contains("\"label\":\"x\""),
        "expected `this` to resolve to Point fields, got: {}",
        json
    );
    assert!(
        json.contains("\"label\":\"y\""),
        "expected field `y`, got: {}",
        json
    );
}

#[test]
fn unknown_receiver_yields_no_members() {
    // `nope` is undeclared; member access on it must return nothing rather than every member.
    let src = "fun main(): void {\n    nope.\n}\n";
    let dot = src.find("nope.").unwrap() + "nope.".len();
    let (line, col) = line_char(src, dot);
    let json = completions(src, line, col);
    assert_eq!(
        json, "[]",
        "unknown receiver should yield no members, got: {}",
        json
    );
}

#[test]
fn formatter_reindents_by_brace_depth() {
    let messy = "fun main(): void {\nlet x: int = 1;\nif (x > 0) {\nprintln(x);\n}\n}\n";
    let formatted = format_document(messy);
    assert!(
        formatted.contains("\n    let x: int = 1;"),
        "expected 4-space indent, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\n        println(x);"),
        "expected nested indent, got:\n{}",
        formatted
    );
}
