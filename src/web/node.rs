pub struct Node {
    pub class: String,
    pub id: String,
    pub key: String,
    pub children: Vec<Node>,
}
impl Node {
    pub fn update(&self) {
        println!("{:?}", self.key);
    }
    pub fn new(key: String) -> Node {
        return Node {
            key: key,
            id: String::from("12"),
            class: String::from("21"),
            children: vec![],
        };
    }
}
