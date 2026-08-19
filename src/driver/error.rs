//! The top-level, typed error returned by [`crate::driver::compiler::Compiler::compile`].
//! `Syntax` / `Semantic` / `Generator` are phase tags for the driver only — user-facing detail
//! is already in the rendered diagnostics. `Io` and `Internal` still carry a message.

use std::fmt;

#[derive(Debug)]
pub enum CompileError {
    /// One or more syntax errors were reported during parsing/import resolution.
    Syntax,
    /// One or more semantic errors were reported during analysis.
    Semantic,
    /// One or more errors from the generate phase (`@json`, syntax DSLs, `@compute` WGSL emit, …).
    Generator,
    /// An I/O failure during the pipeline (reading sources, writing artifacts).
    Io(std::io::Error),
    /// Code generation hit an internal invariant violation (see `crate::internal_error!`) - a
    /// compiler bug on an otherwise-valid program, not a problem with the user's source. Caught at
    /// the top of [`crate::driver::compiler::Compiler::compile`] so it surfaces as a clean message
    /// instead of an unwinding panic with a raw Rust backtrace.
    Internal(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Syntax | CompileError::Semantic | CompileError::Generator => Ok(()),
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
