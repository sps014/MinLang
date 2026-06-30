use super::*;
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;
use pretty_assertions::assert_eq;

fn analyze_code(code: &str) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let arena = bumpalo::Bump::new();
    let mut parser = Parser::new(lexer, &arena, &mut diagnostics);

    if let Ok(tree) = parser.parse() {
        let arena = bumpalo::Bump::new();
        let mut analyzer = Analyzer::new(&tree, &arena);
        let _ = analyzer.analyze(&mut diagnostics);
    }

    diagnostics
}

#[test]
fn test_analyze_valid_types() {
    let code = "fun main(): void { let x: int = 5; let y: float = 3.14; let z: string = \"hello\"; let b: bool = true; }";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), false);
}

#[test]
fn test_analyze_type_mismatch() {
    let code = "fun main(): void { let x: int = \"hello\"; }";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
    assert!(diagnostics
        .diagnostics
        .iter()
        .any(|d| d.message.contains("cannot convert from int to string")));
}

#[test]
fn test_analyze_undefined_variable() {
    let code = "fun main(): void { let x = y + 5; }";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
    assert!(diagnostics
        .diagnostics
        .iter()
        .any(|d| d.message.contains("variable y does not exist")));
}

#[test]
fn test_analyze_array_operations() {
    let code = "
        fun main(): void { 
            let arr: int[] = [1, 2, 3]; 
            let x: int = arr[0];
            arr[1] = 5;
        }
    ";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), false);
}

#[test]
fn test_analyze_invalid_array_operations() {
    let code = "
        fun main(): void { 
            let arr: int[] = [1, 2, 3]; 
            arr[\"hello\"] = 5; // Invalid index type
            let x: int = 5;
            x[0] = 1; // Indexing non-array
        }
    ";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
    assert!(diagnostics
        .diagnostics
        .iter()
        .any(|d| d.message.contains("Array index must be of type int")));
    assert!(diagnostics
        .diagnostics
        .iter()
        .any(|d| d.message.contains("Cannot index into non-array type int")));
}

#[test]
fn test_analyze_async_await_valid() {
    // Calling an async fun yields `Future<T>`; awaiting it (at a statement position) yields `T`.
    let code = "
        async fun delay(): void { }
        async fun work(n: int): int { await delay(); return n * 2; }
        async fun main(): void {
            let h = work(3);
            let v = await h;
            let w = await work(4);
        }
    ";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), false);
}

#[test]
fn test_analyze_await_outside_async() {
    let code = "async fun delay(): int { return 1; } fun main(): void { let x = await delay(); }";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
    assert!(diagnostics.diagnostics.iter().any(|d| d
        .message
        .contains("can only be used inside an 'async' function")));
}

#[test]
fn test_analyze_await_in_subexpression_rejected() {
    // v1 restricts `await` to top-level statement positions.
    let code = "
        async fun delay(): void { }
        async fun work(n: int): int { await delay(); return n; }
        async fun main(): void { let x = await work(1) + 1; }
    ";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
    assert!(diagnostics
        .diagnostics
        .iter()
        .any(|d| d.message.contains("top-level statement")));
}

#[test]
fn test_analyze_await_non_future_rejected() {
    let code = "async fun main(): void { let x = await 5; }";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
}

#[test]
fn test_unresolved_identifier_does_not_cascade() {
    // A single unresolved identifier should report exactly one error: the poison/`Unknown` type it
    // produces unifies with everything, so the downstream `+`, the `: int` annotation, the call
    // argument, and the array index must NOT each add their own follow-on diagnostic.
    let code = "
        fun takes_int(n: int): int { return n; }
        fun main(): void {
            let a: int = missing + 1;
            let b: int = takes_int(missing);
            let arr: int[] = [1, 2, 3];
            let c: int = arr[missing];
        }
    ";
    let diagnostics = analyze_code(code);
    let errors: Vec<&str> = diagnostics
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    // Three uses of `missing`, so three "does not exist" errors -- and nothing else.
    let undefined = errors
        .iter()
        .filter(|m| m.contains("missing does not exist"))
        .count();
    assert_eq!(
        undefined, 3,
        "expected 3 undefined-identifier errors, got: {:?}",
        errors
    );
    assert_eq!(
        errors.len(),
        3,
        "poison type should suppress cascading errors; got: {:?}",
        errors
    );
}

#[test]
fn test_unknown_call_result_does_not_cascade() {
    // Calling an unknown function poisons the result; the inferred variable is poison too, so
    // using it must not pile on more errors.
    let code = "
        fun main(): void {
            let x = nope();
            let y: int = x + 1;
            let z: bool = x;
        }
    ";
    let diagnostics = analyze_code(code);
    let errors: Vec<&str> = diagnostics
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "only the unknown-function error should be reported; got: {:?}",
        errors
    );
}
