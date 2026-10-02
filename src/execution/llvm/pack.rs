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
    remove_bookkeeping(out)
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
