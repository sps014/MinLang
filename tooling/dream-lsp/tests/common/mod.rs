use dream_lsp::index::Index;
use dream_lsp::analysis::{collect_diagnostics, DiagnosticOut};

pub struct TestHarness {
    pub src: String,
    pub offset: usize,
}

impl TestHarness {
    /// Creates a test harness from a source string with a `|` marker indicating the cursor position.
    pub fn new(marked_src: &str) -> Self {
        let offset = marked_src.find('|').expect("Test source must contain a `|` cursor marker");
        let src = marked_src.replace("|", "");
        Self { src, offset }
    }

    /// Builds and returns the symbol Index for the source code.
    pub fn index(&self) -> Index {
        Index::build(None, &self.src)
    }

    /// Runs the diagnostic collector and returns the results.
    pub fn diagnostics(&self) -> Vec<DiagnosticOut> {
        collect_diagnostics(None, &self.src)
    }
}
