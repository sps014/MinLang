use node::Node;
use syntaxtree::*;

#[path = "web/node.rs"]
mod node;

#[path = "rdp/syntaxtree.rs"]
mod syntaxtree;

fn main() {
    let n = Node::new(String::from("abc"));
    n.update();
}
