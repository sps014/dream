pub struct LineText {
    text: String,
    /// Byte offset where each line starts; the first entry is always 0.
    line_starts: Vec<usize>,
}

impl LineText {
    pub fn new(text: String) -> LineText {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(index, _)| index + 1))
            .collect();
        LineText { text, line_starts }
    }

    /// The 1-based line and character column of byte offset `start`. Offsets past the end
    /// resolve on the last line.
    pub fn get_point(&self, start: usize) -> (usize, usize) {
        let line = self.line_starts.partition_point(|&s| s <= start) - 1;
        let line_start = self.line_starts[line];
        let column = match self.text.get(line_start..start) {
            Some(prefix) => prefix.chars().count(),
            None if start > self.text.len() => {
                self.text[line_start..].chars().count() + (start - self.text.len())
            }
            None => start - line_start,
        };
        (line + 1, column + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::LineText;

    #[test]
    fn points_are_one_based_lines_and_columns() {
        let text = LineText::new("ab\ncd\n\nef".into());
        assert_eq!(text.get_point(0), (1, 1));
        assert_eq!(text.get_point(1), (1, 2));
        assert_eq!(text.get_point(2), (1, 3));
        assert_eq!(text.get_point(3), (2, 1));
        assert_eq!(text.get_point(6), (3, 1));
        assert_eq!(text.get_point(7), (4, 1));
        assert_eq!(text.get_point(9), (4, 3));
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let text = LineText::new("é = \"ü\"; x\nλ y".into());
        assert_eq!(text.get_point("é = \"ü\"; ".len()), (1, 10));
        assert_eq!(text.get_point("é = \"ü\"; x\nλ ".len()), (2, 3));
    }

    #[test]
    fn empty_text_and_offsets_past_the_end() {
        assert_eq!(LineText::new(String::new()).get_point(0), (1, 1));
        assert_eq!(LineText::new("ab".into()).get_point(4), (1, 5));
    }
}
