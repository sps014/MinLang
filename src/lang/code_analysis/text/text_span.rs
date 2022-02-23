use std::rc::Rc;
use std::slice::RChunks;
use crate::lang::code_analysis::text::line_text::LineText;

/// a struct representing start and end of a token
/// contains 2 fields of type usize
#[derive(Debug,Copy,Clone)]
pub struct TextSpan
{
    pub start:usize,
    pub end:usize,
    pub line_no:usize,
    pub col_no:usize,
}

impl TextSpan
{
    ///create a new instance of text span from (start,end) tuple
    pub fn new(position:(usize,usize),line_text:&LineText)->Self
    {
        let (line_no,col_no) = line_text.get_point(position.0.clone());
        TextSpan{start:position.0,end:position.1,line_no,col_no}
    }
}