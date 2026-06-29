#[test]
fn test_collect_diagnostics() {
    let src = std::fs::read_to_string("../../src/stdlib/math.dream").unwrap();
    let diagnostics = dream_lsp::analysis::collect_diagnostics(Some("../../src/stdlib/math.dream"), &src);
    for d in diagnostics {
        println!("{:?}", d);
    }
}
