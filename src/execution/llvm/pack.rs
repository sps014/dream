//! `dream pack-runtime <dir>`: the runtime tree a release ships as `lib/dream/rt`, built once by
//! the development LLVM (which has clang) so installs need no C compiler for it. Every flavor ×
//! level × module set is built, since each level compiles the C with its own clang `-O` and the
//! signatures are read from the result; the compiler-rt archives the links need are copied in.

use super::bundle::{ClangRt, FLAVORS, LEVELS, dev_clang_rt, rt_rel_dir};
use super::runtime::build_native_runtime;
use super::tools::resolve_llvm;
use super::wasm::{build_wasm_runtime, flavor};
use dream_mir::runtime::RuntimeNeed;
use std::path::Path;

/// The halves of the runtime tree. The native half (`native/`, `clang_rt/`) is specific to the
/// host; the wasm32 half (`wasm32*/`, `wasi-sysroot/`) is the same on every host, so a release
/// can build it once and merge it into each host's tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PackPart {
    Native,
    Wasm,
}

/// Builds the requested halves of the tree into `out`; `None` builds both.
pub fn pack_runtime(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    out: &Path,
    only: Option<PackPart>,
) -> Result<(), String> {
    let native = only != Some(PackPart::Wasm);
    let wasm = only != Some(PackPart::Native);
    let tools = resolve_llvm(config)?;
    let clang = tools.clang()?;
    super::runtime_cache::freeze_sources();
    let sets = RuntimeNeed::all_sets();
    let total = LEVELS.len() * sets.len();
    let started = std::time::Instant::now();
    for (level, opt) in LEVELS.into_iter().enumerate() {
        for (index, &need) in sets.iter().enumerate() {
            if native {
                build_native_runtime(
                    &tools,
                    &dream_abi::target::TargetSpec::host(),
                    opt,
                    need,
                    &out.join(rt_rel_dir("native", opt, need)),
                )?;
                if opt == crate::driver::wasm_opt::OptLevel::O0 {
                    let objects = super::runtime_objects::native(
                        &tools,
                        &dream_abi::target::TargetSpec::host(),
                        need,
                        false,
                    )?;
                    super::runtime_objects::store_prebuilt(
                        &out.join(rt_rel_dir("native", opt, need))
                            .join("unit-objects"),
                        &objects,
                    )?;
                }
            }
            if wasm {
                for threads in [false, true] {
                    let dir = out.join(rt_rel_dir(flavor(threads), opt, need));
                    build_wasm_runtime(&tools, opt, need, threads, &dir)?;
                    if opt == crate::driver::wasm_opt::OptLevel::O0 {
                        let objects = super::runtime_objects::wasm(&tools, need, threads)?;
                        super::runtime_objects::store_prebuilt(
                            &dir.join("unit-objects"),
                            &objects,
                        )?;
                    }
                }
            }
            // Release CI has no other signal of whether this hours-long step is still advancing.
            eprintln!(
                "pack-runtime: {}/{total} {opt:?} need_{:x} ({:.0?} elapsed)",
                level * sets.len() + index + 1,
                need.bits(),
                started.elapsed()
            );
        }
    }
    if native {
        let crt = out.join("clang_rt");
        std::fs::create_dir_all(&crt).map_err(|e| format!("{}: {e}", crt.display()))?;
        for kind in ClangRt::ALL {
            let src = dev_clang_rt(&clang, kind)?;
            let dst = crt.join(kind.bundled_name());
            std::fs::copy(&src, &dst).map_err(|e| format!("copying {}: {e}", src.display()))?;
        }
    }
    if wasm {
        let sysroot = super::wasm::wasi_sysroot(&clang)?;
        for dir in [
            "include/wasm32-wasip1",
            "lib/wasm32-wasip1",
            "include/wasm32-wasip1-threads",
            "lib/wasm32-wasip1-threads",
        ] {
            copy_tree(&sysroot.join(dir), &out.join("wasi-sysroot").join(dir))?;
        }
    }
    remove_bookkeeping(out)?;
    let flavors: Vec<&str> = FLAVORS
        .into_iter()
        .filter(|&flavor| if flavor == "native" { native } else { wasm })
        .collect();
    remove_unread_signatures(out, &flavors)
}

/// `signatures::load` reads a prebuilt tree's O0 signatures for every level.
fn remove_unread_signatures(out: &Path, flavors: &[&str]) -> Result<(), String> {
    for opt in LEVELS
        .into_iter()
        .filter(|&opt| opt != crate::driver::wasm_opt::OptLevel::O0)
    {
        for need in RuntimeNeed::all_sets() {
            for &flavor in flavors {
                let sigs = out
                    .join(rt_rel_dir(flavor, opt, need))
                    .join("dream_rt.sigs");
                std::fs::remove_file(&sigs).map_err(|e| format!("{}: {e}", sigs.display()))?;
            }
        }
    }
    Ok(())
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
        // Guest links name `lib<name>.a` archives; the shared libraries are never read.
        } else if path.extension().is_none_or(|ext| ext != "so") {
            std::fs::copy(&path, &output)
                .map_err(|e| format!("copying {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Drops the build locks, freshness stamps, dependency lists, signature anchors and the
/// `versions/` snapshots; a prebuilt tree is never rebuilt and is read from its top-level files.
fn remove_bookkeeping(dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str());
        if p.is_dir() {
            if name == Some("versions") {
                std::fs::remove_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            } else {
                remove_bookkeeping(&p)?;
            }
        } else if matches!(
            name,
            Some(
                ".lock"
                    | ".stamp"
                    | "anchor.c"
                    | ".dependencies.json"
                    | ".anchor-dependencies.json"
                    | ".outputs"
            )
        ) {
            std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::wasm_opt::OptLevel;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn pruned_tree_keeps_only_what_prebuilt_lookups_read() {
        let out = tempfile::tempdir().unwrap();
        let out = out.path();
        for opt in LEVELS {
            for need in RuntimeNeed::all_sets() {
                for flavor in FLAVORS {
                    let set = out.join(rt_rel_dir(flavor, opt, need));
                    for name in [
                        "dream_rt.bc",
                        "dream_rt.sigs",
                        "libdream_rt_vendor.a",
                        "0.o",
                        "unit-objects/objects.json",
                        ".lock",
                        ".stamp",
                        ".outputs",
                        ".dependencies.json",
                        ".anchor-dependencies.json",
                        "versions/abc/dream_rt.bc",
                    ] {
                        touch(&set.join(name));
                    }
                }
            }
        }

        remove_bookkeeping(out).unwrap();
        remove_unread_signatures(out, &FLAVORS).unwrap();

        for opt in LEVELS {
            for need in RuntimeNeed::all_sets() {
                for flavor in FLAVORS {
                    let set = out.join(rt_rel_dir(flavor, opt, need));
                    for kept in [
                        "dream_rt.bc",
                        "libdream_rt_vendor.a",
                        "0.o",
                        "unit-objects/objects.json",
                    ] {
                        assert!(set.join(kept).is_file(), "{kept} removed from {set:?}");
                    }
                    for dropped in [
                        ".lock",
                        ".stamp",
                        ".outputs",
                        ".dependencies.json",
                        ".anchor-dependencies.json",
                        "versions",
                    ] {
                        assert!(!set.join(dropped).exists(), "{dropped} kept in {set:?}");
                    }
                    assert_eq!(set.join("dream_rt.sigs").is_file(), opt == OptLevel::O0);
                }
            }
        }
    }

    #[test]
    fn sysroot_copy_skips_shared_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        touch(&source.join("noeh/libc++.a"));
        touch(&source.join("noeh/libc++.so"));
        touch(&source.join("libc.a"));
        let dest = dir.path().join("dest");
        copy_tree(&source, &dest).unwrap();
        assert!(dest.join("noeh/libc++.a").is_file());
        assert!(dest.join("libc.a").is_file());
        assert!(!dest.join("noeh/libc++.so").exists());
    }
}
