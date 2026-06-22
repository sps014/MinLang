use std::collections::HashMap;
use tracing::error;

use crate::syntax::text::text_span::TextSpan;

/// Severity of a reported [`Diagnostic`]. Used to distinguish fatal errors from
/// non-fatal warnings so that callers can decide whether compilation should abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<TextSpan>,
    pub file_path: Option<String>,
}

impl Diagnostic {
    /// Creates an error-severity diagnostic.
    pub fn new(message: String, span: Option<TextSpan>, file_path: Option<String>) -> Self {
        Self { severity: Severity::Error, message, span, file_path }
    }

    /// Creates a warning-severity diagnostic.
    pub fn warning(message: String, span: Option<TextSpan>, file_path: Option<String>) -> Self {
        Self { severity: Severity::Warning, message, span, file_path }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
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

    pub fn report_warning(&mut self, message: String, span: Option<TextSpan>) {
        self.diagnostics.push(Diagnostic::warning(message, span, self.file_path.clone()));
    }

    /// Returns true if at least one error-severity diagnostic has been reported.
    /// Warnings alone do not count as errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn has_warnings(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Warning)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error())
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Warning)
    }

    pub fn extend(&mut self, other: &DiagnosticBag) {
        self.diagnostics.extend(other.diagnostics.clone());
    }
}

/// Renders each diagnostic to the log, including a source-line excerpt with a squiggly
/// underline when the originating file's contents are available. Kept here (rather than in
/// the driver) so diagnostic presentation lives next to the diagnostic data model.
pub fn render(diagnostics: &DiagnosticBag, file_contents: &HashMap<String, String>) {
    for diag in &diagnostics.diagnostics {
        error!("{}", diag.to_string());
        if let (Some(path), Some(span)) = (&diag.file_path, &diag.span) {
            if let Some(content) = file_contents.get(path) {
                let lines: Vec<&str> = content.lines().collect();
                if span.line_no > 0 && span.line_no <= lines.len() {
                    let line_text = lines[span.line_no - 1];
                    error!("  | {}", line_text);
                    let padding = " ".repeat(span.col_no.saturating_sub(1));
                    let squiggly_len = if span.end > span.start { span.end - span.start } else { 1 };
                    let squiggly = "^".repeat(squiggly_len);
                    error!("  | {}{}", padding, squiggly);
                }
            }
        }
    }
}
