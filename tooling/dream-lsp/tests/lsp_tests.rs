mod common;

use common::TestHarness;

#[test]
fn hover_on_function_shows_signature() {
    // We place the cursor | at the start of `add` in the call.
    let src = "
fun add(a: int, b: int): int {
    return a + b;
}
fun main(): void {
    let x: int = |add(1, 2);
    println(x);
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let hover = index.hover(harness.offset).expect("Expected hover info");
    assert!(hover.contents.contains("fun add"));
}

#[test]
fn definition_resolves_call_to_declaration() {
    let src2 = "
fun add(a: int, b: int): int {
    return a + b;
}
fun main(): void {
    let x: int = a|dd(1, 2);
}
";
    let harness = TestHarness::new(src2);
    let index = harness.index();
    
    let def = index.definition(harness.offset).expect("Expected definition");
    
    // The definition of `add` should be near the top.
    // The `add` decl starts at `\nfun add` -> offset is roughly around 5.
    let decl_offset = src2.replace("|", "").find("fun add").unwrap();
    assert!(def.0 >= decl_offset);
}

#[test]
fn diagnostics_flags_unknown_variable() {
    let src = "
fun main(): void {
    let y: int = |nope + 1;
}
";
    let harness = TestHarness::new(src);
    let diagnostics = harness.diagnostics();
    
    let has_error = diagnostics.iter().any(|d| d.severity == "error" && d.message.contains("nope"));
    assert!(has_error, "Expected diagnostic for unknown variable 'nope'");
}

#[test]
fn formatting_reindents_by_brace_depth() {
    let src = "fun main(): void {\nlet x: int = 1;\nif (x > 0) {\nprintln(x);\n}\n}\n";
    let formatted = dream_lsp::format::format(src);
    
    assert!(formatted.contains("\n    let x: int = 1;"));
    assert!(formatted.contains("\n        println(x);"));
}

#[test]
fn completions_include_keywords_and_symbols() {
    let src = "
fun add(a: int, b: int): int {
    return a + b;
}
fun main(): void {
    |
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let comps = index.completions(None, &harness.src, harness.offset);
    
    let has_add = comps.iter().any(|c| c.0 == "add");
    let has_if = comps.iter().any(|c| c.0 == "if");
    
    assert!(has_add, "Expected 'add' in completions");
    assert!(has_if, "Expected 'if' in completions");
}

#[test]
fn member_completions_after_dot() {
    let src = "
class Point {
    x: int;
    fun mag(): int { return this.x; }
}
fun main(): void {
    let origin: Point = Point { x: 0 };
    origin.|
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let comps = index.completions(None, &harness.src, harness.offset);
    
    let has_x = comps.iter().any(|c| c.0 == "x");
    let has_enum = comps.iter().any(|c| c.1 == dream_lsp::index::SymKind::Enum);
    
    assert!(has_x, "Expected 'x' in completions");
    assert!(!has_enum, "Enums should not appear after `.`");
}

#[test]
fn signature_help_on_function() {
    let src = "
fun add(a: int, b: int): int { return a + b; }
fun main(): void {
    add(|);
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let sig = index.signature_help(&harness.src, harness.offset).expect("Expected signature help");
    assert_eq!(sig.name, "add");
}
