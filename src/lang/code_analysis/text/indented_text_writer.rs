pub struct IndentedTextWriter
{
    string:String,
    indent:usize,
    pub indent_string:String,
    indent_pending:bool,
}

impl IndentedTextWriter
{

    pub fn new() -> IndentedTextWriter
    {
        IndentedTextWriter
        {
            string:String::new(),
            indent:0,
            indent_string:String::from("\t"),
            indent_pending:true,
        }
    }
    #[allow(unreachable_code)]
    fn new_line()->&'static str
    {
        #[cfg(windows)]
            return "\r\n";

        return "\n";
    }

    fn indent_string(&mut self)
    {
        if !self.indent_pending
        {
            return;
        }

        for _ in 0..self.indent
        {
            self.string.push_str(&self.indent_string);
        }
        self.indent_pending = false;
    }

    pub fn indent(&mut self)
    {
        self.indent += 1;
    }

    pub fn unindent(&mut self)
    {
        self.indent -= 1;
    }

    pub fn write_line(&mut self, text:&str)
    {
        self.indent_string();
        self.string.push_str(text);
        self.string.push_str(IndentedTextWriter::new_line());
        self.indent_pending=true;
    }
    pub fn write(&mut self, text:&str)
    {
        self.indent_string();
        self.string.push_str(text);
    }
    pub fn to_string(&self) -> String
    {
        self.string.clone()
    }
}