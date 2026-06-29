use std::io::Error;

pub mod wasm;

pub trait CodeGenerator<'a> {
    fn generate(&mut self) -> Result<String, Error>;
}
