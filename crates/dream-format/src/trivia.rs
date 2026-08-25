//! Comment-trivia emission for [`Printer`]: leading/trailing comment placement, blank-line
//! preservation between comment groups, and block-comment re-indentation.

use crate::printer::Printer;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

impl Printer {
    pub(super) fn emit_leading_trivia(&mut self, token: &SyntaxToken) {
        // Blank lines between consecutive leading comments (separated doc-comment groups)
        // are preserved; the gap before the *first* comment is handled by
        // [`Printer::insert_blank_lines`], so tracking starts at `None`.
        let mut last_line: Option<usize> = None;
        for t in &token.leading_trivia {
            if let Some(prev) = last_line {
                let start = self.line_index.line_of(t.position.start);
                if start > prev + 1 && !self.layout.is_empty() {
                    self.layout.blank_line();
                }
            }
            match t.kind {
                TokenKind::LineCommentToken => {
                    if !self.layout.at_line_start() {
                        self.layout.break_line();
                    }
                    self.layout.write_indent(self.plain_indent());
                    self.layout.text(t.text.trim_end());
                    self.layout.break_line();
                }
                TokenKind::BlockCommentToken => {
                    if self.layout.at_line_start() {
                        self.layout.write_indent(self.plain_indent());
                    } else {
                        self.layout.space();
                    }
                    self.write_block_comment(&t.text);
                    self.layout.break_line();
                }
                _ => {}
            }
            last_line = Some(self.line_index.line_of(t.position.end.saturating_sub(1)));
        }
    }

    pub(super) fn emit_trailing_trivia(&mut self, token: &SyntaxToken) {
        for t in &token.trailing_trivia {
            match t.kind {
                TokenKind::LineCommentToken => {
                    self.layout.space();
                    self.layout.text(t.text.trim_end());
                    self.layout.break_line();
                }
                TokenKind::BlockCommentToken => {
                    self.layout.space();
                    self.write_block_comment(&t.text);
                }
                _ => {}
            }
        }
    }

    /// Writes a block comment verbatim, re-indenting only its continuation lines.
    /// Star-aligned doc comments (`/*\n * ... \n */`) are normalized to the canonical
    /// ` *` alignment at the current indent; other comments have their interior base
    /// indentation replaced with the current indent. Both are stable across re-formats.
    fn write_block_comment(&mut self, text: &str) {
        let mut lines = text.split('\n');
        let Some(first) = lines.next() else {
            return;
        };
        self.layout.text(first.trim_end());
        let rest: Vec<&str> = lines.collect();
        if rest.is_empty() {
            return;
        }
        let non_blank: Vec<&&str> = rest.iter().filter(|l| !l.trim().is_empty()).collect();
        let star_aligned = !non_blank.is_empty()
            && non_blank.iter().all(|l| l.trim_start().starts_with('*'));
        let base = non_blank
            .iter()
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        for line in rest {
            self.layout.break_line();
            if line.trim().is_empty() {
                continue;
            }
            self.layout.write_indent(self.plain_indent());
            if star_aligned {
                self.layout.text(" ");
                self.layout.text(line.trim_start().trim_end());
            } else {
                self.layout.text(line[base.min(line.len())..].trim_end());
            }
        }
    }
}
