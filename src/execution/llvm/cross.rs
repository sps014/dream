//! Cross emission compiles the runtime header for its actual target ABI, without linking a
//! host runtime or requiring the foreign libc SDK. Objects keep their runtime symbols unresolved.

use super::runtime::{anchor_unit, reduce_disassembly};
use super::tools::{resolve_llvm, LlvmTools};
use crate::driver::compiler::RuntimeSignatures;
use crate::driver::toolchain::ToolchainConfig;
use crate::driver::wasi::run_captured;
use crate::driver::wasm_opt::OptLevel;
use dream_abi::target::TargetSpec;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn runtime_signatures(
    tools: &LlvmTools,
    target: &TargetSpec,
) -> Result<RuntimeSignatures, String> {
    let dir = tools
        .config
        .prefix
        .join("cache/llvm-cross")
        .join(target.triple.to_string());
    // Signature compilation is cheap, and rebuilding prevents a stale ABI after a header edit.
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("build.lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let clang = tools.clang()?;
    let command = |source: &Path| {
        let mut cmd = std::process::Command::new(&clang);
        cmd.arg(format!("--target={}", target.triple))
            .args([
                "-std=gnu11",
                "-ffreestanding",
                "-DDREAM_NATIVE",
                "-DDREAM_ALWAYS_INLINE=__attribute__((always_inline))",
                "-w",
                "-ferror-limit=0",
            ])
            .arg(format!(
                "-I{}",
                dream_mir::runtime::native_runtime_include_dir(&tools.config.runtime_c).display()
            ))
            .arg(source);
        cmd
    };
    let check = |source: &Path| {
        let output = command(source)
            .arg("-fsyntax-only")
            .output()
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&output.stderr).into_owned())
    };
    let compile = |source: &Path, output: &Path| {
        run_captured(
            command(source)
                .args(["-emit-llvm", "-c", "-O0"])
                .arg("-o")
                .arg(output),
            "cross runtime ABI header",
        )
    };
    let anchor = anchor_unit(&dir, "dream_rt_native.h", &check, &compile)?;
    let ir = dir.join("anchor.ll");
    run_captured(
        tools.command("llvm-dis").arg(&anchor).arg("-o").arg(&ir),
        "cross ABI disassembly",
    )?;
    let text = reduce_disassembly(&std::fs::read_to_string(ir).map_err(|e| e.to_string())?);
    let cache_path = dir.join("dream_rt.sigs");
    std::fs::write(&cache_path, &text).map_err(|e| e.to_string())?;
    Ok(RuntimeSignatures { text, cache_path })
}

pub fn emit_object(
    config: &Arc<ToolchainConfig>,
    target: &TargetSpec,
    ir: &Path,
    level: OptLevel,
) -> Result<PathBuf, String> {
    let tools = resolve_llvm(config)?;
    let object = ir.with_extension("o");
    run_captured(
        tools
            .command("opt")
            .args(["-passes=verify", "-disable-output"])
            .arg(ir),
        "cross IR verification",
    )?;
    // Never select the runner's CPU for an object intended for another machine.
    run_captured(
        tools
            .command("llc")
            .arg(format!("-mtriple={}", target.triple))
            .arg(super::build::llc_level(level, false))
            .args(["-filetype=obj", "-relocation-model=pic"])
            .arg(ir)
            .arg("-o")
            .arg(&object),
        "cross object emission",
    )?;
    Ok(object)
}
