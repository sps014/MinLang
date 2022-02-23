use crate::lang::code_analysis::text::text_span::TextSpan;

pub struct LineText
{
    text: String,
    line_width: Vec<usize>
}

impl LineText {
    pub fn new(text: String) -> LineText {

        LineText {
            text: text.clone(),
            line_width: LineText::calculate_line_width(text)
        }
    }
    ///internally used to calculate the line widths of the text
    fn calculate_line_width(input:String)->Vec<usize>
    {
        let mut line_width = Vec::new();
        let mut width = 0;
        let parts=input.split("\n");
        for c in input.chars() {
            if c == '\n' {
                line_width.push(width+1);
                width = 0;
            }
            else {
                width += 1;
            }
        }
        line_width.push(width+1);
        line_width
    }
    ///returns the line number,column number of the token at the given index
    pub fn get_point(&self,start:usize)->(usize,usize)
    {
        let mut line_number=0;
        let mut sum:usize=0;
        // visit each line width
        for i in self.line_width.iter()
        {
            //if the sum+current_line_size is greater than the start index, then we are in the line
            if sum+*i>start
            {
                break;
            }
            sum+=*i;
            line_number+=1;
        }
        (line_number+1,if start>sum {start-sum+1} else{ sum-start+1})
    }

}