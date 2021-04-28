use syntaxtree::*;
#[path = "syntaxtree.rs"]
mod syntaxtree;

pub struct Parser {
    pub tree: SytaxTree,
}
impl Parser {
    pub fn match_str(parts: Vec<&str>) -> bool {
        true
    }
}
