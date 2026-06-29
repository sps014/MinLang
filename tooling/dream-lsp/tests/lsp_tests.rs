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
    
    let hover = index.hover(harness.offset, &src).expect("Expected hover info");
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
    let origin: Point = Point(0);
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

#[test]
fn diagnostics_missing_semicolon_position() {
    let src = "
fun main(): void {
    let y: int = 1|
    let x: int = 2;
}
";
    let harness = TestHarness::new(src);
    let diagnostics = harness.diagnostics();
    
    let has_error = diagnostics.iter().any(|d| d.severity == "error" && d.message.contains("Expected ';'"));
    assert!(has_error, "Expected missing semicolon error");
}

#[test]
fn diagnostics_flags_type_mismatch() {
    let src = "
fun main(): void {
    let y: int = |\"hello\";
}
";
    let harness = TestHarness::new(src);
    let diagnostics = harness.diagnostics();
    
    let has_error = diagnostics.iter().any(|d| d.severity == "error" && d.message.contains("cannot convert"));
    assert!(has_error, "Expected diagnostic for type mismatch");
}

#[test]
fn hover_on_struct_field() {
    let src = "
class User {
    age: int;
}
fun main(): void {
    let u: User = User(20);
    let a: int = u.|age;
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let hover = index.hover(harness.offset, &src).expect("Expected hover info");
    assert!(hover.contents.contains("int"), "Hover should contain field type");
}

#[test]
fn signature_help_second_parameter() {
    let src = "
fun add(a: int, b: int): int { return a + b; }
fun main(): void {
    add(1, |);
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let sig = index.signature_help(&harness.src, harness.offset).expect("Expected signature help");
    assert_eq!(sig.name, "add");
}

#[test]
fn inferred_type_member_completion_forward_reference() {
    let src = "
fun main(): void {
    let u = User(20, \"Alice\");
    u.|
}

class User {
    age: int;
    name: string;
    fun get_age(): int { return this.age; }
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let comps = index.completions(None, &harness.src, harness.offset);
    let has_age = comps.iter().any(|c| c.0 == "age");
    assert!(has_age, "Expected 'age' in completions");
}

#[test]
fn hover_inferred_variable() {
    let src = "
fun main(): void {
    let u = User(20);
    u|
}
class User { age: int; }
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    // Check hover for the reference
    let hover_ref = index.hover(harness.offset - 1, &src).expect("Expected hover info on ref");
    assert!(hover_ref.contents.contains("User"), "Hover should show User type on ref, got {}", hover_ref.contents);
}

#[test]
fn hover_inferred_variable_after_error() {
    let src = "
fun main(): void {
    let x: int = 1 + ; // ERROR HERE
    let u = User(20);
    u|
}
class User { age: int; }
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    // Check hover for the reference
    let hover_ref = index.hover(harness.offset - 1, &src).expect("Expected hover info on ref");
    assert!(hover_ref.contents.contains("User"), "Hover should show User type on ref, got {}", hover_ref.contents);
}
#[test]
fn explicit_type_hint_cross_file_inference() {
    let dir = std::env::temp_dir().join("dream_lsp_tests2");
    std::fs::create_dir_all(&dir).unwrap();

    let other_file = dir.join("other.dream");
    std::fs::write(&other_file, "
class RemoteUser {
    id: int;
    fun get_id(): int { return this.id; }
}
fun fetch_user(): RemoteUser {
    return RemoteUser(42);
}
").unwrap();

    let main_src = "
import \"other\";

fun main(): void {
    let u: RemoteUser = fetch_user();
    u.|
}
";
    let main_file = dir.join("main.dream");
    std::fs::write(&main_file, main_src).unwrap();

    let offset = main_src.find('|').unwrap();
    let src = main_src.replace("|", "");

    let index = dream_lsp::index::Index::build(Some(main_file.to_str().unwrap()), &src);

    let comps = index.completions(Some(main_file.to_str().unwrap()), &src, offset);
    println!("Completions: {:?}", comps);

    let has_id = comps.iter().any(|c| c.0 == "id");
    let has_get_id = comps.iter().any(|c| c.0 == "get_id");

    assert!(has_id, "Expected 'id' in completions");
    assert!(has_get_id, "Expected 'get_id' in completions");
}

#[test]
fn explicit_test_class_cross_file_inference() {
    let dir = std::env::temp_dir().join("dream_lsp_tests_3");
    std::fs::create_dir_all(&dir).unwrap();

    let other_file = dir.join("basic_sum.dream");
    std::fs::write(&other_file, "
public fun add_numbers(a: int, b: int): int {
    return a + b;
}

public class Test {
    public name: string;
    public age: int;

    constructor(name: string, age: int) {
        this.name = name;
        this.age = age;
    }

    public fun print_name() {
        println(this.name);
    }
}
").unwrap();

    let main_src = "
import \"basic_sum.dream\"

public fun main() {
    let result = add_numbers(10,20);
    let t = Test(\"John\", 20);
    t.|
}
";
    let main_file = dir.join("main.dream");
    std::fs::write(&main_file, main_src).unwrap();

    let offset = main_src.find('|').unwrap();
    let src = main_src.replace("|", "");

    let index = dream_lsp::index::Index::build(Some(main_file.to_str().unwrap()), &src);

    // Check variable `t` type
    let decl_t = index.decls.iter().find(|d| d.name == "t").unwrap();
    assert_eq!(decl_t.ty, Some("Test".to_string()));

    // Check variable `result` type
    let decl_res = index.decls.iter().find(|d| d.name == "result").unwrap();
    assert_eq!(decl_res.ty, Some("int".to_string()));

    let comps = index.completions(Some(main_file.to_str().unwrap()), &src, offset);

    let has_name = comps.iter().any(|c| c.0 == "name");
    let has_print_name = comps.iter().any(|c| c.0 == "print_name");

    assert!(has_name, "Expected 'name' in completions");
    assert!(has_print_name, "Expected 'print_name' in completions");
}

#[test]
fn hover_on_builtin_list_push() {
    let src = "
fun main(): void {
    let xs = List<int>();
    xs.pu|sh(1);
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    
    let hover = index.hover(harness.offset, &src).expect("Expected hover info on builtin method");
    println!("HOVER CONTENTS: {}", hover.contents);
    // With generic substitution, it should show 'push(value: int)' instead of 'push(value: T)'
    assert!(hover.contents.contains("push(value: int)"));
    assert!(hover.contents.contains("Appends a value to the end"));
}

#[test]
fn test_hover_math_floor() {
    let src = "
fun main(): void {
    let m = Math.f|loor(3.7);
}
";
    let harness = TestHarness::new(src);
    let index = harness.index();
    let hover = index.hover(harness.offset, &src).expect("Expected hover info on Math.floor");
    println!("HOVER CONTENTS MATH.FLOOR: {}", hover.contents);
}

#[test]
fn parameter_inlay_hints_on_function_and_constructor_calls() {
    use dream_lsp::index::{Index, InlayKind};
    let src = "
class Point {
    x: int;
    y: int;
}
fun add(a: int, b: int): int {
    return a + b;
}
fun main(): void {
    let p = Point(3, 4);
    let s = add(1, 2);
}
";
    let index = Index::build(None, src);
    let labels: Vec<&str> = index
        .inlay_hints
        .iter()
        .filter(|h| h.kind == InlayKind::Parameter)
        .map(|h| h.label.as_str())
        .collect();
    // Auto-generated constructor takes the struct's fields positionally.
    assert!(labels.contains(&"x:"), "expected `x:` hint, got {:?}", labels);
    assert!(labels.contains(&"y:"), "expected `y:` hint, got {:?}", labels);
    // Free function parameters.
    assert!(labels.contains(&"a:"), "expected `a:` hint, got {:?}", labels);
    assert!(labels.contains(&"b:"), "expected `b:` hint, got {:?}", labels);
}

#[test]
fn parameter_inlay_hints_suppressed_when_arg_matches_name() {
    use dream_lsp::index::{Index, InlayKind};
    let src = "
fun add(a: int, b: int): int {
    return a + b;
}
fun main(): void {
    let a = 1;
    let b = 2;
    let s = add(a, b);
}
";
    let index = Index::build(None, src);
    let param_hints = index
        .inlay_hints
        .iter()
        .filter(|h| h.kind == InlayKind::Parameter)
        .count();
    assert_eq!(
        param_hints, 0,
        "argument identifiers matching parameter names should not be annotated"
    );
}

#[test]
fn parameter_inlay_hints_on_method_calls() {
    use dream_lsp::index::{Index, InlayKind};
    let src = "
fun main(): void {
    let nums = List<int>();
    nums.push(42);
}
";
    let index = Index::build(None, src);
    let labels: Vec<&str> = index
        .inlay_hints
        .iter()
        .filter(|h| h.kind == InlayKind::Parameter)
        .map(|h| h.label.as_str())
        .collect();
    assert!(
        labels.contains(&"value:"),
        "expected `value:` hint on List.push, got {:?}",
        labels
    );
}
