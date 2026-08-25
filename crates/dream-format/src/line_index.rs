/// Byte-offset → source-line lookup for blank-line preservation decisions.
pub(crate) struct LineIndex {
    /// Byte offset of the first character of each line; always starts with `0`.
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex { line_starts }
    }

    /// 0-based line containing byte offset `offset` (clamped to the last line).
    pub fn line_of(&self, offset: usize) -> usize {
        self.line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }
}
