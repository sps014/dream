use super::INDENT_UNIT;

/// Line-oriented output buffer. Newlines and indentation are structural decisions — there is
/// never any need to rewind emitted text, which is what keeps `} else {` joining, blank-line
/// insertion, and idempotency free of character-surgery hacks.
pub(super) struct Layout {
    lines: Vec<String>,
    cur: String,
}

impl Layout {
    pub fn new() -> Self {
        Layout {
            lines: Vec::new(),
            cur: String::new(),
        }
    }

    pub fn text(&mut self, s: &str) {
        self.cur.push_str(s);
    }

    /// Appends a single separating space unless the line is empty or already space-terminated.
    pub fn space(&mut self) {
        if !self.cur.is_empty() && !self.cur.ends_with(' ') {
            self.cur.push(' ');
        }
    }

    pub fn at_line_start(&self) -> bool {
        self.cur.is_empty()
    }

    /// Writes `level` indent units, but only onto a fresh line.
    pub fn write_indent(&mut self, level: usize) {
        if self.cur.is_empty() {
            for _ in 0..level {
                self.cur.push_str(INDENT_UNIT);
            }
        }
    }

    /// Terminates the current line. A no-op when the line is empty, so stacked break
    /// decisions (`;` then `}` then a fresh statement) collapse to one newline.
    pub fn break_line(&mut self) {
        if !self.cur.is_empty() {
            let line = std::mem::take(&mut self.cur);
            self.lines.push(line.trim_end().to_string());
        }
    }

    /// Inserts exactly one empty line between the previous content line and the next.
    pub fn blank_line(&mut self) {
        self.break_line();
        self.lines.push(String::new());
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.cur.is_empty()
    }

    /// True when the output already sits on an empty line with a blank above it — used to
    /// avoid stacking a second blank.
    pub fn has_pending_blank(&self) -> bool {
        self.cur.is_empty() && self.lines.last().map(|l| l.is_empty()).unwrap_or(false)
    }

    /// Flushes the current line, drops trailing blanks, and guarantees one trailing newline.
    pub fn finish(mut self) -> String {
        self.break_line();
        while self.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            self.lines.pop();
        }
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }
}
