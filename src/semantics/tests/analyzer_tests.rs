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
    assert!(diagnostics.diagnostics.iter().any(|d| d.message.contains("cannot convert from int to string")));
}

#[test]
fn test_analyze_undefined_variable() {
    let code = "fun main(): void { let x = y + 5; }";
    let diagnostics = analyze_code(code);
    assert_eq!(diagnostics.has_errors(), true);
    assert!(diagnostics.diagnostics.iter().any(|d| d.message.contains("variable y does not exist")));
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
    assert!(diagnostics.diagnostics.iter().any(|d| d.message.contains("Array index must be of type int")));
    assert!(diagnostics.diagnostics.iter().any(|d| d.message.contains("Cannot index into non-array type int")));
}
