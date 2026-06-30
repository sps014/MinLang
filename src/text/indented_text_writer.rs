pub struct IndentedTextWriter {
    string: String,
    indent: usize,
    pub indent_string: String,
    indent_pending: bool,
}

impl Default for IndentedTextWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl IndentedTextWriter {
    pub fn new() -> IndentedTextWriter {
        IndentedTextWriter {
            string: String::new(),
            indent: 0,
            indent_string: String::from("\t"),
            indent_pending: true,
        }
    }
    fn new_line() -> &'static str {
        if cfg!(windows) {
            "\r\n"
        } else {
            "\n"
        }
    }

    fn indent_string(&mut self) {
        if !self.indent_pending {
            return;
        }

        for _ in 0..self.indent {
            self.string.push_str(&self.indent_string);
        }
        self.indent_pending = false;
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn unindent(&mut self) {
        self.indent -= 1;
    }

    pub fn write_line(&mut self, text: &str) {
        self.indent_string();
        self.string.push_str(text);
        self.string.push_str(IndentedTextWriter::new_line());
        self.indent_pending = true;
    }
    pub fn write(&mut self, text: &str) {
        self.indent_string();
        self.string.push_str(text);
    }
    /// Writes a multi-line block, applying the current indentation to each line.
    pub fn write_block(&mut self, text: &str) {
        for line in text.lines() {
            self.write_line(line);
        }
    }
}

impl std::fmt::Display for IndentedTextWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}
