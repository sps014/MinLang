use elementref::Node;

#[path ="web/elementref.rs"] mod elementref;
fn main() {
    elementref::df();
    let n:elementref::Node=Node{cool:21};
    n.update();
}
