//! User-facing build output for the `dream` CLI: cargo-style colored status lines, artifact
//! summaries with sizes, and rustc-style `error:` / `warning:` / `help:` notes. Colors follow
//! the same TTY / `NO_COLOR` / `CLICOLOR_FORCE` rules as diagnostics rendering.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";

/// Width the status label column is right-aligned into (cargo's convention).
const LABEL_WIDTH: usize = 12;

/// True when user-facing CLI output should use ANSI colors (TTY stderr, no `NO_COLOR`).
pub fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(std::env::var_os("CLICOLOR_FORCE"), Some(v) if v != "0") {
        return true;
    }
    std::io::stderr().is_terminal()
}

/// Progress + warning sink used by [`crate::driver::compiler::Compiler`]. The default impl is
/// silent so library users (tests, LSP) get no console output; the CLI installs
/// [`ConsoleReporter`], which buffers artifact paths and renders everything through [`Ui`].
pub trait BuildReporter: Send + Sync {
    fn artifact(&self, _path: &Path) {}
    fn warning(&self, _msg: &str) {}
}

/// No-op reporter — the [`Compiler`](crate::driver::compiler::Compiler) default.
pub struct SilentReporter;

impl BuildReporter for SilentReporter {}

/// Reporter that records emitted artifacts and surfaces warnings as styled stderr lines.
pub struct ConsoleReporter {
    artifacts: Mutex<Vec<PathBuf>>,
}

impl ConsoleReporter {
    pub fn new() -> Self {
        Self {
            artifacts: Mutex::new(Vec::new()),
        }
    }

    /// Drains the recorded artifact paths (for the end-of-build summary).
    pub fn take_artifacts(&self) -> Vec<PathBuf> {
        std::mem::take(&mut self.artifacts.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Default for ConsoleReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildReporter for ConsoleReporter {
    fn artifact(&self, path: &Path) {
        self.artifacts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(path.to_path_buf());
    }

    fn warning(&self, msg: &str) {
        Ui::new().warning(msg);
    }
}

/// Renders status lines and diagnostics to stderr.
pub struct Ui {
    color: bool,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            color: color_enabled(),
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    /// One cargo-style progress line: a right-aligned green verb plus its detail.
    pub fn step(&self, label: &str, detail: &str) {
        let pad = LABEL_WIDTH.saturating_sub(label.len());
        eprintln!(
            "{}{} {}",
            " ".repeat(pad),
            self.paint(&format!("{BOLD}{GREEN}"), label),
            detail
        );
    }

    /// A dim informational note (e.g. the unoptimized-debug-build reminder).
    pub fn note(&self, msg: &str) {
        eprintln!("{}", self.paint(DIM, msg));
    }

    /// A `help:` line following an error or note.
    pub fn help(&self, msg: &str) {
        eprintln!("{}", self.paint(BLUE, &format!("help: {msg}")));
    }

    pub fn error(&self, msg: &str) {
        eprintln!(
            "{} {}",
            self.paint(&format!("{BOLD}{RED}"), "error:"),
            self.paint(BOLD, msg)
        );
    }

    /// Multi-line error: header first, then indented captured tool output (e.g. clang stderr).
    pub fn error_with_detail(&self, msg: &str, detail: &str) {
        self.error(msg);
        for line in detail.lines() {
            eprintln!("  {}", self.paint(DIM, line));
        }
    }

    pub fn warning(&self, msg: &str) {
        eprintln!(
            "{} {}",
            self.paint(&format!("{BOLD}{YELLOW}"), "warning:"),
            msg
        );
    }

    /// Bold-green success line (e.g. the `dream test` summary).
    pub fn success(&self, msg: &str) {
        eprintln!("{}", self.paint(&format!("{BOLD}{GREEN}"), msg));
    }

    /// End-of-build summary: a green `Finished` line followed by the artifact table with sizes.
    pub fn finish(&self, elapsed_secs: f64, detail: &str, artifacts: &[PathBuf]) {
        self.step(
            "Finished",
            &format!(
                "{}in {:.2}s",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!("{detail} ")
                },
                elapsed_secs
            ),
        );
        if !artifacts.is_empty() {
            let width = artifacts
                .iter()
                .map(|p| p.display().to_string().len())
                .max()
                .unwrap_or(0);
            for path in artifacts {
                let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                let pad = width.saturating_sub(path.display().to_string().len());
                eprintln!(
                    "  {}{} {}",
                    path.display(),
                    " ".repeat(pad),
                    self.paint(DIM, &format_size(size))
                );
            }
        }
    }

    /// Friendly reminder that a non-`--release`/`-O` build ships unoptimized code. Wasm wording
    /// points at download size instead of runtime speed.
    pub fn debug_build_note(&self, wasm: bool) {
        eprintln!();
        self.note("note: this is an unoptimized debug build");
        if wasm {
            self.help("run with --release or -Os to shrink the .wasm for production");
        } else {
            self.help("run with --release for a faster, optimized binary");
        }
    }

    /// Cyan accent for values inside step details (kept minimal — labels carry the color).
    pub fn accent(&self, text: &str) -> String {
        self.paint(CYAN, text)
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

/// Human-readable byte size (`842 B`, `12.3 KiB`, `1.21 MiB`, …).
pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let b = bytes as f64;
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if b < KIB * KIB {
        return format!("{:.1} KiB", b / KIB);
    }
    if b < KIB * KIB * KIB {
        return format!("{:.2} MiB", b / (KIB * KIB));
    }
    format!("{:.2} GiB", b / (KIB * KIB * KIB))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(842), "842 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(12 * 1024 + 512), "12.5 KiB");
        assert_eq!(format_size(1536 * 1024), "1.50 MiB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.00 GiB");
    }

    #[test]
    fn silent_reporter_records_nothing() {
        // SilentReporter must compile as a BuildReporter with all-noop behavior.
        let r = SilentReporter;
        r.artifact(Path::new("x"));
        r.warning("w");
    }
}
