use super::*;

pub(super) fn fail_diagnostics(
    ctor: fn(String) -> CompileError,
    diagnostics: &DiagnosticBag,
    file_contents: &std::collections::HashMap<String, String>,
) -> CompileError {
    render_with(diagnostics, file_contents, Some(highlight_dream_line));
    ctor(format_diagnostics(
        diagnostics,
        file_contents,
        false,
        Some(highlight_dream_line),
    ))
}

/// Extracts a human-readable message from a caught panic payload (the `Any` that
/// `std::panic::catch_unwind` hands back), covering the two shapes `panic!`/`internal_error!`
/// actually produce (`&'static str` and `String`).
pub(super) fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "internal compiler error: codegen panicked with a non-string payload".to_string()
    }
}

/// Prints a caught codegen panic the way [`render`] prints ordinary diagnostics, so an internal
/// compiler error looks like the rest of the CLI's output rather than a raw Rust panic dump.
pub(super) fn render_internal_error(message: &str) {
    eprintln!("error: {}", message);
}
