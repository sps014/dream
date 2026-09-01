//! LSP glue for the Dream formatter (the engine lives in the `dream-format` crate).
//!
//! [`minimal_edits`] diffs the original and formatted documents and returns a single
//! whole-line-range `TextEdit` covering just the differing region, so clients do not replace
//! the entire buffer when only a few lines changed.

use crate::conversions::map_position;
use crate::position::LineIndex;
use dream_format::format as format_document;
use tower_lsp::lsp_types::{Range, TextEdit};

/// Formats `text` and returns `Some(edits)` only when formatting changes it.
pub fn formatting_edits(text: &str) -> Option<Vec<TextEdit>> {
    let formatted = format_document(text);
    if formatted == text {
        return None;
    }
    Some(minimal_edits(text, &formatted))
}

/// Computes one whole-line-range `TextEdit` transforming `old` into `new`.
///
/// Both documents are compared as `\n`-split segments; the shared leading and trailing runs
/// are trimmed away and the remaining old range is replaced by the remaining new segments.
pub fn minimal_edits(old: &str, new: &str) -> Vec<TextEdit> {
    let old_segs: Vec<&str> = old.split('\n').collect();
    let new_segs: Vec<&str> = new.split('\n').collect();

    let mut start = 0usize;
    while start < old_segs.len() && start < new_segs.len() && old_segs[start] == new_segs[start] {
        start += 1;
    }
    let mut end_old = old_segs.len();
    let mut end_new = new_segs.len();
    while end_old > start && end_new > start && old_segs[end_old - 1] == new_segs[end_new - 1] {
        end_old -= 1;
        end_new -= 1;
    }
    if end_old == start && end_new == start {
        return Vec::new();
    }

    let line_index = LineIndex::new(old);
    let range = Range {
        start: map_position(line_index.position(offset_of_line(&old_segs, start))),
        end: map_position(line_index.position(if end_old >= old_segs.len() {
            old.len()
        } else {
            offset_of_line(&old_segs, end_old)
        })),
    };
    // Replacing up to (not including) the final segment also swallows the newline that
    // terminates the last replaced segment, so it must be re-added.
    let mut new_text = new_segs[start..end_new].join("\n");
    if end_old < old_segs.len() {
        new_text.push('\n');
    }
    vec![TextEdit { range, new_text }]
}

fn offset_of_line(segs: &[&str], line: usize) -> usize {
    segs[..line.min(segs.len())]
        .iter()
        .map(|l| l.len() + 1)
        .sum()
}
