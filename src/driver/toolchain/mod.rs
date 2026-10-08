//! One environment snapshot shared by compilation, linking and generator harnesses.

mod environment;
mod fingerprint;
mod paths;
#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(feature = "native")]
use std::sync::OnceLock;

pub struct ToolchainConfig {
    pub runtime_c: PathBuf,
    pub home: Option<PathBuf>,
    pub bin: Option<PathBuf>,
    pub llvm: Option<PathBuf>,
    pub wasm_opt: Option<PathBuf>,
    pub toolchains: Vec<PathBuf>,
    pub prefix: PathBuf,
    pub user_home: Option<PathBuf>,
    pub exe: Option<PathBuf>,
    pub cwd: PathBuf,
    pub path: Vec<PathBuf>,
    pub cc: Option<OsString>,
    pub cxx: Option<OsString>,
    pub zig: Option<PathBuf>,
    pub targets: PathBuf,
    pub sysroot: Option<PathBuf>,
    pub no_auto_install: bool,
    pub native_sanitize: Option<OsString>,
    pub runtime_counters: bool,
    pub asan_options: Option<OsString>,
    pub loader_path: Option<OsString>,
    #[cfg(feature = "native")]
    pub(crate) sdkroot_args: std::sync::Mutex<std::collections::BTreeMap<String, Vec<String>>>,
    pub sdkroot: Option<OsString>,
    pub developer_dir: Option<OsString>,
    pub compiler_environment: std::collections::BTreeMap<String, OsString>,
    #[cfg(windows)]
    pub windir: Option<PathBuf>,
    #[cfg(feature = "native")]
    pub(crate) resolved_llvm: OnceLock<Result<PathBuf, String>>,
    #[cfg(feature = "native")]
    pub(crate) resolved_wasm_opt: OnceLock<Result<PathBuf, String>>,
    #[cfg(feature = "native")]
    pub(crate) resolved_cc: OnceLock<Result<crate::execution::native::cc::Cc, String>>,
}
