//! `dream pack-runtime <dir>`: the runtime tree a release ships as `lib/dream/rt`, built once by
//! the development LLVM (which has clang) so installs need no C compiler for it. Every flavor ×
//! level × module set is built, since each level compiles the C with its own clang `-O` and the
//! signatures are read from the result; the compiler-rt archives the links need are copied in.

use super::bundle::{dev_clang_rt, rt_rel_dir, ClangRt, LEVELS};
use super::runtime::build_native_runtime;
use super::tools::resolve_llvm;
use super::wasm::{build_wasm_runtime, flavor};
use dream_mir::runtime::RuntimeNeed;
use std::path::Path;

pub fn pack_runtime(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    out: &Path,
) -> Result<(), String> {
    let tools = resolve_llvm(config)?;
    let clang = tools.clang()?;
    for opt in LEVELS {
        for need in RuntimeNeed::all_sets() {
            build_native_runtime(
                &tools,
                &dream_abi::target::TargetSpec::host(),
                opt,
                need,
                &out.join(rt_rel_dir("native", opt, need)),
            )?;
            for threads in [false, true] {
                let dir = out.join(rt_rel_dir(flavor(threads), opt, need));
                build_wasm_runtime(&tools, opt, need, threads, &dir)?;
            }
        }
    }
    let crt = out.join("clang_rt");
    std::fs::create_dir_all(&crt).map_err(|e| format!("{}: {e}", crt.display()))?;
    for kind in ClangRt::ALL {
        let src = dev_clang_rt(&clang, kind)?;
        let dst = crt.join(kind.bundled_name());
        std::fs::copy(&src, &dst).map_err(|e| format!("copying {}: {e}", src.display()))?;
    }
    let sysroot = super::wasm::wasi_sysroot(&clang)?;
    for dir in ["include/wasm32-wasip1", "lib/wasm32-wasip1"] {
        copy_tree(&sysroot.join(dir), &out.join("wasi-sysroot").join(dir))?;
    }
    remove_bookkeeping(out)
}

fn copy_tree(source: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut entries = std::fs::read_dir(source)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let output = dest.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &output)?;
        } else {
            std::fs::copy(&path, &output)
                .map_err(|e| format!("copying {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Drops the build locks, freshness stamps and signature anchors; a prebuilt tree is never
/// rebuilt.
fn remove_bookkeeping(dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            remove_bookkeeping(&p)?;
        } else if matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some(".lock" | ".stamp" | "anchor.c")
        ) {
            std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}
