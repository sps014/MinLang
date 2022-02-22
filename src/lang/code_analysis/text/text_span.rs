/// a struct representing start and end of a token
/// contains 2 fields of type usize
#[derive(Debug,Copy,Clone)]
pub struct TextSpan
{
    start:usize,
    end:usize
}

impl TextSpan
{
    ///create a new instance of text span from (start,end) tuple
    pub fn new(position:(usize,usize))->Self
    {
        TextSpan{start:position.0,end:position.1}
    }
}