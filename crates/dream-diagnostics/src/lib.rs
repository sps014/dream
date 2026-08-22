use dream_text::text_span::TextSpan;

mod render;
pub use render::{color_enabled, format_diagnostics, render, render_with};

/// Severity of a reported [`Diagnostic`]. Used to distinguish fatal errors from
/// non-fatal warnings so that callers can decide whether compilation should abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<TextSpan>,
    pub file_path: Option<String>,
    /// Follow-up lines rendered after the source excerpt (`note:` dim, `help:` blue).
    pub notes: Vec<DiagnosticNote>,
}

/// One follow-up line attached to a diagnostic.
#[derive(Debug, Clone)]
pub struct DiagnosticNote {
    pub kind: NoteKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Note,
    Help,
}

impl Diagnostic {
    /// Creates an error-severity diagnostic.
    pub fn new(message: String, span: Option<TextSpan>, file_path: Option<String>) -> Self {
        Self {
            severity: Severity::Error,
            message,
            span,
            file_path,
            notes: Vec::new(),
        }
    }

    /// Creates a warning-severity diagnostic.
    pub fn warning(message: String, span: Option<TextSpan>, file_path: Option<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message,
            span,
            file_path,
            notes: Vec::new(),
        }
    }

    /// Builder: appends a `help:` follow-up line.
    pub fn with_help(mut self, message: impl Into<String>) -> Self {
        self.notes.push(DiagnosticNote {
            kind: NoteKind::Help,
            message: message.into(),
        });
        self
    }

    /// Builder: appends a `note:` follow-up line.
    pub fn with_note(mut self, message: impl Into<String>) -> Self {
        self.notes.push(DiagnosticNote {
            kind: NoteKind::Note,
            message: message.into(),
        });
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.file_path {
            write!(f, "{}: ", path)?;
        }
        if let Some(span) = &self.span {
            write!(f, "{} ", span.get_point_str())?;
        }
        write!(f, "{}", self.message)
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticBag {
    pub diagnostics: Vec<Diagnostic>,
    pub file_path: Option<String>,
}

impl DiagnosticBag {
    pub fn new(file_path: Option<String>) -> Self {
        Self {
            diagnostics: Vec::new(),
            file_path,
        }
    }

    /// Reports a pre-built diagnostic (builder-style: `Diagnostic::new(..).with_help(..)`).
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn report_error(&mut self, message: String, span: Option<TextSpan>) {
        self.diagnostics
            .push(Diagnostic::new(message, span, self.file_path.clone()));
    }

    pub fn report_warning(&mut self, message: String, span: Option<TextSpan>) {
        self.diagnostics
            .push(Diagnostic::warning(message, span, self.file_path.clone()));
    }

    /// Returns true if at least one error-severity diagnostic has been reported.
    /// Warnings alone do not count as errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error())
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    pub fn extend(&mut self, other: &DiagnosticBag) {
        self.diagnostics.extend(other.diagnostics.clone());
    }
}
