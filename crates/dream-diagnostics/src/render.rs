use std::collections::HashMap;
use std::io::IsTerminal;

use crate::{Diagnostic, DiagnosticBag, Severity};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";
const DIM: &str = "\x1b[2m";

/// True when user-facing diagnostics should use ANSI colors (TTY stderr, no `NO_COLOR`).
pub fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(std::env::var_os("CLICOLOR_FORCE"), Some(v) if v != "0") {
        return true;
    }
    std::io::stderr().is_terminal()
}

/// Writes rustc-style diagnostics to stderr.
pub fn render(diagnostics: &DiagnosticBag, file_contents: &HashMap<String, String>) {
    render_with(diagnostics, file_contents, None);
}

/// Like [`render`], with an optional source-line highlighter (keywords/strings/…).
pub fn render_with(
    diagnostics: &DiagnosticBag,
    file_contents: &HashMap<String, String>,
    highlight: Option<fn(&str, bool) -> String>,
) {
    let color = color_enabled();
    eprint!(
        "{}",
        format_diagnostics(diagnostics, file_contents, color, highlight)
    );
}

/// Formats the bag the same way [`render_with`] prints it. `color` is explicit so tests do not
/// depend on whether stderr is a TTY.
pub fn format_diagnostics(
    diagnostics: &DiagnosticBag,
    file_contents: &HashMap<String, String>,
    color: bool,
    highlight: Option<fn(&str, bool) -> String>,
) -> String {
    let mut out = String::new();
    // Group diagnostics by file (first-seen file order, within-file order preserved) so
    // multi-file errors render as one contiguous block per file instead of interleaving
    // excerpts back and forth.
    let mut file_order: Vec<Option<String>> = Vec::new();
    let mut by_file: Vec<Vec<&Diagnostic>> = Vec::new();
    fn index_of(file: &Option<String>, order: &mut Vec<Option<String>>) -> usize {
        if let Some(pos) = order.iter().position(|f| f == file) {
            return pos;
        }
        order.push(file.clone());
        order.len() - 1
    }
    for diag in &diagnostics.diagnostics {
        let idx = index_of(&diag.file_path, &mut file_order);
        if by_file.len() <= idx {
            by_file.resize_with(idx + 1, Vec::new);
        }
        by_file[idx].push(diag);
    }
    for (i, (_, diags)) in file_order.iter().zip(by_file.iter()).enumerate() {
        if i > 0 && !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        for diag in diags {
            format_one(&mut out, diag, file_contents, color, highlight);
        }
    }
    out
}

fn paint(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn format_one(
    out: &mut String,
    diag: &Diagnostic,
    file_contents: &HashMap<String, String>,
    color: bool,
    highlight: Option<fn(&str, bool) -> String>,
) {
    let (level, level_code) = match diag.severity {
        Severity::Error => ("error", format!("{BOLD}{RED}")),
        Severity::Warning => ("warning", format!("{BOLD}{YELLOW}")),
    };
    out.push_str(&paint(color, &level_code, level));
    out.push_str(&paint(color, BOLD, ": "));
    out.push_str(&paint(color, BOLD, &diag.message));
    out.push('\n');

    let Some(path) = &diag.file_path else {
        out.push('\n');
        return;
    };
    let span = diag.span.filter(|s| s.line_no > 0);
    let loc = match span {
        Some(s) => format!("{path}:{}:{}", s.line_no, s.col_no.max(1)),
        None => path.clone(),
    };
    out.push_str(&paint(color, &format!("{BOLD}{CYAN}"), " --> "));
    out.push_str(&loc);
    out.push('\n');

    let Some(span) = span else {
        out.push('\n');
        return;
    };
    let Some(content) = file_contents.get(path) else {
        out.push('\n');
        return;
    };
    let lines: Vec<&str> = content.lines().collect();
    if span.line_no == 0 || span.line_no > lines.len() {
        out.push('\n');
        return;
    }
    let line_text = lines[span.line_no - 1];
    let line_no = span.line_no.to_string();
    let gutter = line_no.len();
    let bar = paint(color, &format!("{BOLD}{CYAN}"), "|");
    out.push_str(&format!("{:>gutter$} {bar}\n", ""));
    let shown = match highlight {
        Some(h) => h(line_text, color),
        None => line_text.to_string(),
    };
    out.push_str(&paint(color, &format!("{BOLD}{CYAN}"), &line_no));
    out.push_str(&format!(" {bar} {shown}\n"));

    let col = span.col_no.max(1);
    let start_col = col.saturating_sub(1);
    let span_len = if span.end > span.start {
        span.end - span.start
    } else {
        1
    };
    let max_len = line_text.len().saturating_sub(start_col).max(1);
    let squiggly_len = span_len.min(max_len).max(1);
    let padding = " ".repeat(start_col);
    let squiggly = "^".repeat(squiggly_len);
    let caret = paint(color, &format!("{BOLD}{RED}"), &squiggly);
    out.push_str(&format!("{:>gutter$} {bar} {padding}{caret}\n", ""));

    for note in &diag.notes {
        let label = match note.kind {
            crate::NoteKind::Help => ("help:", format!("{BOLD}{BLUE}")),
            crate::NoteKind::Note => ("note:", DIM.to_string()),
        };
        out.push_str(&format!(
            "{} {}\n",
            paint(color, &label.1, label.0),
            paint(color, DIM, &note.message)
        ));
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagnosticBag;
    use dream_text::line_text::LineText;
    use dream_text::text_span::TextSpan;
    use std::rc::Rc;

    #[test]
    fn rustc_layout_without_color() {
        let mut bag = DiagnosticBag::new(Some("demo.dream".to_string()));
        let src = "fun main() {\n    let x: int = \"hi\";\n}\n";
        let lt = Rc::new(LineText::new(src.to_string()));
        // "hi" starts around the assignment; pick a span on line 2.
        let span = TextSpan::new(
            (src.find("\"hi\"").unwrap(), src.find("\"hi\"").unwrap() + 4),
            &lt,
        );
        bag.report_error("cannot convert from string to int".to_string(), Some(span));
        let mut files = HashMap::new();
        files.insert("demo.dream".to_string(), src.to_string());
        let rendered = format_diagnostics(&bag, &files, false, None);
        assert!(rendered.starts_with("error: cannot convert from string to int\n"));
        assert!(rendered.contains(" --> demo.dream:2:"));
        assert!(rendered.contains("2 |     let x: int = \"hi\";"));
        assert!(rendered.contains('^'));
        assert!(!rendered.contains('\u{1b}'));
    }
}
