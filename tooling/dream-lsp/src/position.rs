//! Maps the compiler's byte-offset spans onto LSP/Monaco positions, which are 0-based
//! line numbers paired with UTF-16 code-unit columns.

use serde::Serialize;

/// A 0-based line / UTF-16 column position, matching the LSP and Monaco position model.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// A half-open `[start, end)` range expressed in [`Position`]s.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Precomputed line-start byte offsets for a document, enabling O(log n) conversion from a
/// byte offset to a (line, UTF-16 column) position. Built once per request from the source text.
pub struct LineIndex {
    text: String,
    /// Byte offset at which each line begins. `line_starts[0]` is always 0.
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            text: text.to_string(),
            line_starts,
        }
    }

    /// Converts a byte offset into a 0-based line and UTF-16 column. Offsets past the end of
    /// the document clamp to the final position so synthesized spans never panic.
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        };
        let line_start = self.line_starts[line];
        let column = self.utf16_len(&self.text[line_start..offset]);
        Position {
            line: line as u32,
            character: column as u32,
        }
    }

    pub fn range(&self, start: usize, end: usize) -> Range {
        Range {
            start: self.position(start),
            end: self.position(end),
        }
    }

    /// Converts a 0-based (line, UTF-16 column) position back into a byte offset, clamping
    /// out-of-range input to the document bounds.
    pub fn offset(&self, line: u32, character: u32) -> usize {
        let line = line as usize;
        if line >= self.line_starts.len() {
            return self.text.len();
        }
        let line_start = self.line_starts[line];
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        let mut remaining = character as usize;
        let mut offset = line_start;
        for ch in self.text[line_start..line_end].chars() {
            if remaining == 0 {
                break;
            }
            let units = ch.len_utf16();
            if remaining < units {
                break;
            }
            remaining -= units;
            offset += ch.len_utf8();
        }
        offset.min(line_end)
    }

    fn utf16_len(&self, s: &str) -> usize {
        s.chars().map(|c| c.len_utf16()).sum()
    }
}
