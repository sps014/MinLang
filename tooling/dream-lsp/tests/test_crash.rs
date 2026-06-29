#[test]
fn test_crash() {
    let src = std::fs::read_to_string("../../src/stdlib/math.dream").unwrap();
    let idx = dream_lsp::index::Index::build(Some("../../src/stdlib/math.dream"), &src);
    println!("Built idx successfully. Decls: {}", idx.decls.len());
}
