//! A conservative source formatter for Dream. Rather than round-tripping the AST (which would
//! risk dropping comments and trailing detail), it reindents the document by brace depth while
//! preserving each line's content. The scanner is aware of string/char literals and line/block
//! comments so braces inside them never affect indentation.

const INDENT_UNIT: &str = "    ";

#[derive(Clone, Copy, PartialEq)]
enum State {
    Normal,
    String,
    Char,
    LineComment,
    Block,
}

/// Reindents `text`, returning the formatted document. Trailing whitespace is trimmed and a
/// single trailing newline is ensured.
pub fn format(text: &str) -> String {
    let mut depth: i32 = 0;
    let mut in_block = false;
    let mut out = String::new();

    let lines: Vec<&str> = text.split('\n').collect();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end();

        if in_block {
            // Inside a multi-line block comment: pass content through verbatim and only watch
            // for the closing delimiter.
            out.push_str(line);
            in_block = !block_closes(line);
        } else {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                // leave blank line empty
            } else {
                let first_is_close = trimmed.starts_with('}')
                    || trimmed.starts_with(')')
                    || trimmed.starts_with(']');
                let this_depth = (depth - if first_is_close { 1 } else { 0 }).max(0);
                for _ in 0..this_depth {
                    out.push_str(INDENT_UNIT);
                }
                out.push_str(trimmed);

                let scan = scan_line(trimmed);
                depth = (depth + scan.delta).max(0);
                in_block = scan.ends_in_block;
            }
        }

        if i + 1 < lines.len() {
            out.push('\n');
        }
    }

    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

struct LineScan {
    /// Net `{`/`}` (and `(`/`)`, `[`/`]`) balance contributed by this line, ignoring literals
    /// and comments.
    delta: i32,
    /// Whether the line ends inside an unterminated block comment.
    ends_in_block: bool,
}

/// Scans a single line that begins in normal code, counting bracket balance while skipping
/// string/char literals and comments.
fn scan_line(line: &str) -> LineScan {
    let mut state = State::Normal;
    let mut delta = 0i32;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match state {
            State::Normal => match c {
                '{' | '(' | '[' => delta += 1,
                '}' | ')' | ']' => delta -= 1,
                '"' => state = State::String,
                '\'' => state = State::Char,
                '/' => match chars.peek() {
                    Some('/') => {
                        chars.next();
                        state = State::LineComment;
                    }
                    Some('*') => {
                        chars.next();
                        state = State::Block;
                    }
                    _ => {}
                },
                _ => {}
            },
            State::String => match c {
                '\\' => {
                    chars.next();
                }
                '"' => state = State::Normal,
                _ => {}
            },
            State::Char => match c {
                '\\' => {
                    chars.next();
                }
                '\'' => state = State::Normal,
                _ => {}
            },
            State::LineComment => break,
            State::Block => {
                if c == '*' {
                    if let Some('/') = chars.peek() {
                        chars.next();
                        state = State::Normal;
                    }
                }
            }
        }
    }

    LineScan {
        delta,
        ends_in_block: state == State::Block,
    }
}

/// Whether a line already inside a block comment contains the closing `*/`.
fn block_closes(line: &str) -> bool {
    let mut prev = '\0';
    for c in line.chars() {
        if prev == '*' && c == '/' {
            return true;
        }
        prev = c;
    }
    false
}
