//! Diagnostics tests: clean code is quiet, broken code is flagged, and—crucially for editing—
//! semantic diagnostics keep flowing even while the document has a syntax error (the parser
//! recovers and the analyzer runs over whatever parsed).

mod common;

use common::TestHarness;

#[test]
fn clean_program_has_no_errors() {
    let harness = TestHarness::new(
        "fun main(): void {\n    let x: int = 1;\n    let y: int = x + 2;\n}\n|",
    );
    let diagnostics = harness.diagnostics();
    assert!(
        diagnostics.iter().all(|d| d.severity != "error"),
        "expected no errors for a clean program, got: {:?}",
        diagnostics
    );
}

#[test]
fn unknown_identifier_is_flagged() {
    let harness = TestHarness::new("fun main(): void {\n    let y: int = nope + 1;\n}\n|");
    let diagnostics = harness.diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.severity == "error" && d.message.contains("nope")),
        "expected an error mentioning the unknown identifier, got: {:?}",
        diagnostics
    );
}

#[test]
fn semantic_diagnostics_survive_a_syntax_error() {
    // The first function has a syntax error; the batch compiler would stop here and report nothing
    // about `main`. The editor instead recovers and still flags the undefined `nope` in `main`.
    let harness = TestHarness::new(
        "fun broken(): void {\n    let a: int = ;\n}\nfun main(): void {\n    let y: int = nope + 1;\n}\n|",
    );
    let diagnostics = harness.diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.message.contains("nope")),
        "expected semantic diagnostics under syntax-error recovery, got: {:?}",
        diagnostics
    );
}
