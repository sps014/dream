//! Turns uses of moved locals into diagnostics that say where the value went and how to keep
//! using it.

use super::state::{Destination, Status};
use super::*;
use dream_diagnostics::Diagnostic;
use dream_hir::HSpan;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;

/// Source lines per file, built on first use.
struct Sources<'g> {
    files: &'g [crate::module_graph::SourceFile<'g>],
    lines: IndexMap<String, LineText>,
}

impl Sources<'_> {
    fn span(&mut self, file: &str, span: HSpan) -> Option<TextSpan> {
        if span.is_none() {
            return None;
        }
        if !self.lines.contains_key(file) {
            let source = self.files.iter().find(|f| f.path == file)?.source.clone();
            self.lines.insert(file.to_string(), LineText::new(source));
        }
        let lines = self.lines.get(file)?;
        Some(TextSpan::new(
            (span.start as usize, span.end as usize),
            lines,
        ))
    }
}

impl<'a> Analyzer<'a> {
    pub(super) fn report_moves(
        &self,
        found: Vec<FunctionFindings<'_>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let mut sources = Sources {
            files: &self.graph.files,
            lines: IndexMap::new(),
        };
        // Generic instances share one source body; report each use once.
        let mut reported: IndexSet<(String, HSpan)> = IndexSet::new();
        for FunctionFindings { function, findings } in found {
            let Some(file) = function.file.as_deref() else {
                continue;
            };
            let bindings = super::bindings(function);
            for finding in findings {
                if !reported.insert((file.to_string(), finding.span)) {
                    continue;
                }
                let Some(&(name, ty)) = bindings.get(&finding.local) else {
                    continue;
                };
                let site = finding.status.site();
                let line = sources
                    .span(file, site.span)
                    .map(|s| format!(" on line {}", s.line_no))
                    .unwrap_or_default();
                let in_loop = site.span == finding.span;
                let message = match finding.status {
                    _ if in_loop => {
                        format!("'{name}' is moved here on every pass through the loop")
                    }
                    Status::Moved(_) => format!("use of '{name}' after it was moved"),
                    Status::MaybeMoved(_) => {
                        format!("use of '{name}', which may have been moved")
                    }
                };
                let moved = match finding.status {
                    _ if in_loop => "was moved on an earlier pass",
                    Status::Moved(_) => "was moved",
                    Status::MaybeMoved(_) => "may have been moved",
                };
                let diagnostic = Diagnostic::new(
                    message,
                    sources.span(file, finding.span),
                    Some(file.to_string()),
                )
                .with_code("use-after-move");
                let (note, owner) = match &site.to {
                    Destination::Argument { callee, param } if !param.is_empty() => (
                        format!(
                            "'{name}' {moved} into '{callee}'{line}, which keeps its '{param}' argument"
                        ),
                        None,
                    ),
                    Destination::Argument { callee, .. } => {
                        (format!("'{name}' {moved} into '{callee}'{line}"), None)
                    }
                    Destination::Local(to) => {
                        (format!("'{name}' {moved} into '{to}'{line}"), Some(to))
                    }
                    Destination::Place(Some(place)) => (
                        format!("'{name}' {moved} into '{place}'{line}; '{place}' owns it now"),
                        Some(place),
                    ),
                    Destination::Place(None) => (
                        format!("'{name}' {moved} into a field or element{line}"),
                        None,
                    ),
                    Destination::Aggregate => {
                        (format!("'{name}' {moved} into a new value{line}"), None)
                    }
                };
                let diagnostic = diagnostic.with_note(note);
                // Pointing at the new owner only helps when every path reaching this use moved
                // the value there.
                let diagnostic = match owner {
                    Some(owner) if !in_loop && matches!(finding.status, Status::Moved(_)) => {
                        diagnostic.with_help(format!("use '{owner}' here instead of '{name}'"))
                    }
                    _ => diagnostic,
                };
                let diagnostic = if in_loop {
                    diagnostic.with_help(format!(
                        "create '{name}' inside the loop so each pass moves its own value"
                    ))
                } else {
                    diagnostic.with_help(format!(
                        "or read what you need from '{name}' before the line that moves it"
                    ))
                };
                let diagnostic = diagnostic.with_help(self.ownership_help(name, ty));
                diagnostics.report(diagnostic);
            }
        }
    }
}
