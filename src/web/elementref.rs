pub fn df() {
    println!("Call script from other folders");
}
pub struct Node {
    pub cool: i32,
}
impl Node {
    pub fn update(&self) {
        println!("{:?}",self.cool);
    }
}
