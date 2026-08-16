//! Flags forwarded to clang: LTO, sanitizers, WASM `-mattr`, DWARF, C ABI.

use super::triple::{host_triple, Triple};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lto {
    None,
    Thin,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sanitize {
    None,
    Address,
}

#[derive(Debug, Clone)]
pub struct CodegenOptions {
    pub triple: Triple,
    pub opt_level: u8,
    pub lto: Lto,
    pub sanitize: Sanitize,
    pub debug_info: bool,
    /// Link a shared library (DAP loads it in-process) instead of an executable.
    pub link_shared: bool,
    /// WASM feature attrs, e.g. `+bulk-memory,+simd128,+tail-call`.
    pub mattr: String,
    pub sysroot: Option<String>,
    /// `ccc` is the default C calling convention clang applies to `extern "C"` IR decls.
    pub c_calling_conv: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        CodegenOptions {
            triple: host_triple(),
            opt_level: 0,
            lto: Lto::None,
            sanitize: Sanitize::None,
            debug_info: false,
            link_shared: false,
            mattr: "+bulk-memory,+simd128".to_string(),
            sysroot: None,
            c_calling_conv: true,
        }
    }
}

impl CodegenOptions {
    pub fn release(mut self) -> Self {
        self.opt_level = 3;
        if self.lto == Lto::None {
            self.lto = Lto::Thin;
        }
        self
    }
}
