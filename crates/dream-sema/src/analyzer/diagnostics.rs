use super::*;

/// Reports `message` at `span` into the bag and returns the matching typed [`SemanticError`], so a
/// failing analysis site can `return Err(report(diagnostics, msg, span))` in a single step. The
/// pushed diagnostic is what the user sees; the returned error drives `?`-based short-circuiting of
/// the rest of the offending expression.
pub(super) fn report(
    diagnostics: &mut DiagnosticBag,
    message: String,
    span: Option<TextSpan>,
) -> SemanticError {
    diagnostics.report_error(message.clone(), span);
    SemanticError::reported(message, span)
}

/// Like [`report`], attaching follow-up `help:` lines (e.g. did-you-mean suggestions) and an
/// optional stable machine-readable error-class code (see [`report_with_code`]).
pub(super) fn report_noted(
    diagnostics: &mut DiagnosticBag,
    message: String,
    span: Option<TextSpan>,
    notes: Vec<String>,
    code: Option<&'static str>,
) -> SemanticError {
    let mut diag = Diagnostic::new(message.clone(), span, diagnostics.file_path.clone());
    if let Some(code) = code {
        diag = diag.with_code(code);
    }
    for n in notes {
        diag = diag.with_help(n);
    }
    diagnostics.report(diag);
    SemanticError::reported(message, span)
}

/// Like [`report`], attaching a stable machine-readable error-class code (`unresolved-name`,
/// `missing-member`, …) so tooling (the LSP's auto-import code action) can react to this
/// specific failure without sniffing message text.
pub(super) fn report_with_code(
    diagnostics: &mut DiagnosticBag,
    message: String,
    span: Option<TextSpan>,
    code: &'static str,
) -> SemanticError {
    let diag =
        Diagnostic::new(message.clone(), span, diagnostics.file_path.clone()).with_code(code);
    diagnostics.report(diag);
    SemanticError::reported(message, span)
}

/// Reports `Type '{owner}' has no {kind} '{member}'`. When an opt-in std package adds that
/// member through an `extend` block, the help names the missing `import` instead of `similar`.
pub(super) fn report_missing_member(
    diagnostics: &mut DiagnosticBag,
    (owner, kind, member): (&str, &str, &str),
    span: Option<TextSpan>,
    similar: Vec<String>,
) -> SemanticError {
    let base = owner.split('<').next().unwrap_or(owner);
    let notes = match dream_stdlib::imports::package_extending(base, member) {
        Some(package) => vec![format!(
            "'{base}.{member}' comes from package '{package}'; add 'import {package};'"
        )],
        None => similar,
    };
    report_noted(
        diagnostics,
        format!("Type '{owner}' has no {kind} '{member}'"),
        span,
        notes,
        Some("missing-member"),
    )
}

/// The `help:` note naming the opt-in std package that declares the top-level `name`, if any.
pub(super) fn import_note(name: &str) -> Option<String> {
    dream_stdlib::imports::package_exporting(name)
        .map(|package| format!("'{name}' comes from package '{package}'; add 'import {package};'"))
}
