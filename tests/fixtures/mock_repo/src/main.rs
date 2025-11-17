pub fn main() { println!("Mock repo"); }

pub fn test_fn() -> i32 { 42 }

#[derive(Debug)]
pub struct MockStruct;

impl MockStruct {
    pub fn method(&self) -> &str { "test" }
}
