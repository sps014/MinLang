use crate::lang::code_analysis::text::text_span::TextSpan;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Option<TextSpan>,
    pub file_path: Option<String>,
}

impl Diagnostic {
    pub fn new(message: String, span: Option<TextSpan>, file_path: Option<String>) -> Self {
        Self { message, span, file_path }
    }
    
    pub fn to_string(&self) -> String {
        let mut result = String::new();
        if let Some(path) = &self.file_path {
            result.push_str(&format!("{}: ", path));
        }
        if let Some(span) = &self.span {
            result.push_str(&format!("{} ", span.get_point_str()));
        }
        result.push_str(&self.message);
        result
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticBag {
    pub diagnostics: Vec<Diagnostic>,
    pub file_path: Option<String>,
}

impl DiagnosticBag {
    pub fn new(file_path: Option<String>) -> Self {
        Self { diagnostics: Vec::new(), file_path }
    }

    pub fn report_error(&mut self, message: String, span: Option<TextSpan>) {
        self.diagnostics.push(Diagnostic::new(message, span, self.file_path.clone()));
    }

    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
    
    pub fn extend(&mut self, other: &DiagnosticBag) {
        self.diagnostics.extend(other.diagnostics.clone());
    }
}
