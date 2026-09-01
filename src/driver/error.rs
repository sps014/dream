//! The top-level, typed error returned by [`crate::driver::compiler::Compiler::compile`].
//! `Syntax` / `Semantic` / `Generator` are phase tags for the driver only — user-facing detail
//! is already in the rendered diagnostics (also stored here so tests can assert on the text).
//! `Io` and `Internal` still carry a message.

use std::fmt;

#[derive(Debug)]
pub enum CompileError {
    /// One or more syntax errors were reported during parsing/import resolution.
    Syntax(String),
    /// One or more semantic errors were reported during analysis.
    Semantic(String),
    /// One or more errors from the generate phase (`@json`, syntax DSLs, `@compute` WGSL emit, …).
    Generator(String),
    /// An I/O failure during the pipeline (reading sources, writing artifacts).
    Io(std::io::Error),
    /// Code generation hit an internal invariant violation (see `crate::internal_error!`) - a
    /// compiler bug on an otherwise-valid program, not a problem with the user's source. Caught
    /// around analysis and code generation in [`crate::driver::compiler::Compiler::compile`] so it
    /// surfaces as a clean message instead of an unwinding panic with a raw Rust backtrace.
    Internal(String),
}

impl CompileError {
    /// Colorless diagnostic text when this error came from the user-facing pipeline.
    pub fn diagnostic_text(&self) -> Option<&str> {
        match self {
            CompileError::Syntax(s)
            | CompileError::Semantic(s)
            | CompileError::Generator(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Syntax(_) | CompileError::Semantic(_) | CompileError::Generator(_) => {
                Ok(())
            }
            CompileError::Io(e) => write!(f, "{}", e),
            CompileError::Internal(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<std::io::Error> for CompileError {
    fn from(e: std::io::Error) -> Self {
        CompileError::Io(e)
    }
}
