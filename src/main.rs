use node::Node;
use parser::Parser;
use syntaxtree::*;

#[path = "web/node.rs"]
mod node;

#[path = "rdp/syntaxtree.rs"]
mod syntaxtree;

#[path = "rdp/parser.rs"]
mod parser;

fn main() {
    let parts = vec!["<", "bc", ">", "ok", "</", "bc", ">"];
    let result = Parser::match_str(parts);
    print!("ok");
}
