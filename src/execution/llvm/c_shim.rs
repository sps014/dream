//! Compiles a program's generated `@c` shim (`dream_abi::c_abi::shim`) to bitcode for the build
//! target, so clang lowers every C prototype with the target's own ABI rules. The bitcode is linked
//! into the program module before `opt`, which inlines the forward shims away.

use super::runtime::strip_target_cpu;
use super::tools::LlvmTools;
use crate::driver::compiler::c_shim_path;
use crate::driver::wasi::run_captured;
use dream_abi::target::TargetSpec;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The shim bitcode for the program at `ll_path`, or `None` when it calls no C.
pub(super) fn shim_bitcode(
    tools: &LlvmTools,
    spec: &TargetSpec,
    ll_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let src = c_shim_path(ll_path);
    if !src.exists() {
        return Ok(None);
    }
    let out = src.with_extension("bc");
    let mut cmd = Command::new(tools.clang()?);
    // `-disable-llvm-passes` keeps clang from tagging functions `optnone`; `opt` optimizes the
    // linked program at its own level. `-fno-builtin` keeps a bound libc name (`free`, `strlen`)
    // an ordinary call under whatever prototype the extern gave it.
    cmd.arg(format!("--target={}", spec.llvm_triple()))
        .args([
            "-std=gnu11",
            "-ffreestanding",
            "-fno-builtin",
            "-w",
            "-c",
            "-emit-llvm",
            "-O1",
            "-Xclang",
            "-disable-llvm-passes",
        ])
        .arg(&src)
        .arg("-o")
        .arg(&out);
    // A non-PIC shim's module flags can force local-exec TLS in the linked runtime.
    if !spec.is_windows() && !spec.capabilities.linear_memory {
        cmd.arg("-fPIC");
    }
    run_captured(&mut cmd, &format!("clang ({})", src.display()))?;
    strip_target_cpu(tools, &out)?;
    Ok(Some(out))
}
