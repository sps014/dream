//! What a release ships under `lib/dream/` beside the `dream` binary: the minimal LLVM
//! (`llvm/bin`), and the runtime prebuilt for every level and module set (`rt/`, written by
//! `dream pack-runtime`), including the compiler-rt archives the links need (`rt/clang_rt/`).
//!
//! Only the directory next to the running executable counts, so a development build never picks
//! up an installed runtime that was built from different C sources.

use crate::driver::toolchain::ToolchainConfig;
use crate::driver::wasm_opt::OptLevel;
use dream_mir::runtime::RuntimeNeed;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `lib/dream` of the running executable: `<exe dir>/lib/dream` (unpacked archive) or
/// `<exe dir>/../lib/dream` (installed `bin/` layout).
pub fn bundled_llvm_bin(config: &ToolchainConfig) -> Option<PathBuf> {
    config.bundle_dir().map(|d| d.join("llvm/bin"))
}

pub fn prebuilt_rt(config: &ToolchainConfig) -> Option<PathBuf> {
    config.bundle_dir().map(|d| d.join("rt"))
}

/// Runtime flavors, each a `<flavor>/<level>/need_<bits>/` tree.
pub const FLAVORS: [&str; 3] = ["native", "wasm32", "wasm32-threads"];

/// Every level a runtime is built at (`O4` shares `O3`'s runtime).
pub const LEVELS: [OptLevel; 6] = [
    OptLevel::O0,
    OptLevel::O1,
    OptLevel::O2,
    OptLevel::O3,
    OptLevel::Size,
    OptLevel::SizeAggressive,
];

pub fn rt_rel_dir(flavor: &str, opt: OptLevel, need: RuntimeNeed) -> PathBuf {
    Path::new(flavor)
        .join(opt.native_rt_subdir())
        .join(format!("need_{:x}", need.bits()))
}

/// A runtime flavor's directory: read-only and complete in a release, else the development
/// cache it is built into.
pub enum RtDir {
    Prebuilt(PathBuf),
    Cache(PathBuf),
}

pub fn rt_dir(config: &ToolchainConfig, flavor: &str, opt: OptLevel, need: RuntimeNeed) -> RtDir {
    let rel = rt_rel_dir(flavor, opt, need);
    let rel = if config.runtime_counters {
        PathBuf::from("runtime-counters").join(rel)
    } else {
        rel
    };
    match (!config.runtime_counters)
        .then(|| prebuilt_rt(config))
        .flatten()
    {
        Some(root) => RtDir::Prebuilt(root.join(rel)),
        None => RtDir::Cache(
            config
                .native_rt_cache_root()
                .join(format!("llvm-{}", super::LLVM_VERSION))
                .join(rel),
        ),
    }
}

/// `file` inside a prebuilt flavor directory, which a release must contain.
pub fn prebuilt_file(dir: &Path, file: &str) -> Result<PathBuf, String> {
    let p = dir.join(file);
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!("this Dream install is missing {}", p.display()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClangRt {
    /// compiler-rt builtins for the wasm32 guest (`__multi3`, …).
    WasmBuiltins { threads: bool },
    /// The instrumentation runtime a `--profile` binary links.
    Profile,
}

impl ClangRt {
    pub const ALL: [ClangRt; 3] = [
        ClangRt::WasmBuiltins { threads: false },
        ClangRt::WasmBuiltins { threads: true },
        ClangRt::Profile,
    ];

    /// File name inside `rt/clang_rt/`.
    pub fn bundled_name(self) -> &'static str {
        match self {
            ClangRt::WasmBuiltins { threads: false } => "wasm32.builtins.a",
            ClangRt::WasmBuiltins { threads: true } => "wasm32-threads.builtins.a",
            ClangRt::Profile if cfg!(windows) => "profile.lib",
            ClangRt::Profile => "profile.a",
        }
    }
}

/// Where `kind` lives: under `rt/clang_rt/` in a release, else in the resource directory of the
/// development LLVM's clang (`scripts/fetch-dev-llvm.sh` adds the wasm32 builtins there).
pub fn clang_rt(tools: &super::LlvmTools, kind: ClangRt) -> Result<PathBuf, String> {
    if let Some(rt) = prebuilt_rt(&tools.config) {
        return prebuilt_file(&rt.join("clang_rt"), kind.bundled_name());
    }
    dev_clang_rt(&tools.clang()?, kind)
}

pub(super) fn dev_clang_rt(clang: &Path, kind: ClangRt) -> Result<PathBuf, String> {
    let query = |args: &[&str]| -> Result<PathBuf, String> {
        let out = Command::new(clang)
            .args(args)
            .output()
            .map_err(|e| format!("failed to query {}: {e}", clang.display()))?;
        Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
    };
    let path = match kind {
        ClangRt::WasmBuiltins { threads } => query(&[
            if threads {
                "--target=wasm32-wasip1-threads"
            } else {
                "--target=wasm32-wasip1"
            },
            "-print-libgcc-file-name",
        ])?,
        ClangRt::Profile => {
            let dir = query(&["-print-runtime-dir"])?;
            [
                "libclang_rt.profile_osx.a",
                "libclang_rt.profile.a",
                "clang_rt.profile-x86_64.lib",
                "clang_rt.profile.lib",
            ]
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file())
            .unwrap_or(dir)
        }
    };
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "compiler-rt archive not found at {}; run scripts/fetch-dev-llvm.sh",
            path.display()
        ))
    }
}
