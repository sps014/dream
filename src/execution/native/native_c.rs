//! Compiles a program's live `native/` C/C++ source sets (the `.abi.json` `c_sources` list) to
//! objects with the same toolchain that links the binary (Zig by default, so C++ objects and the
//! final link agree on one libc++). Objects are cached per set under `<artifact dir>/native-c/`,
//! rebuilt when a source, a header under an include dir, or the flags change.

use super::cc::Cc;
use crate::driver::wasi::run_captured;
use dream_mir::runtime::runtime_abi_include_dir;
use serde::Deserialize;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CSourceSet {
    pub name: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub libs: Vec<String>,
    /// Runtime functions the set's code calls (generated `@cpp` shims), kept exported by `opt`.
    #[serde(default)]
    pub runtime_exports: Vec<String>,
}

#[derive(Deserialize)]
struct AbiFile {
    #[serde(default)]
    c_sources: Vec<CSourceSet>,
}

pub fn read_c_sources_from_abi(abi_path: &Path) -> Vec<CSourceSet> {
    let Ok(text) = std::fs::read_to_string(abi_path) else {
        return Vec::new();
    };
    serde_json::from_str::<AbiFile>(&text)
        .map(|a| a.c_sources)
        .unwrap_or_default()
}

/// Everything the link step needs from the compiled sets.
#[derive(Debug, Default)]
pub struct NativeObjects {
    pub objects: Vec<PathBuf>,
    pub link_args: Vec<String>,
    pub needs_cxx: bool,
    pub runtime_exports: Vec<String>,
}

fn is_cxx(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("cpp" | "cc" | "cxx")
    )
}

/// Compiles every set to objects under `cache_root/<set>/`.
pub fn compile_sets(
    config: &crate::driver::toolchain::ToolchainConfig,
    cc: &Cc,
    spec: &dream_abi::target::TargetSpec,
    sets: &[CSourceSet],
    cache_root: &Path,
    debug: bool,
) -> Result<NativeObjects, String> {
    let mut out = NativeObjects::default();
    let embed_include = runtime_abi_include_dir(&config.runtime_c);
    for set in sets {
        let dir = cache_root.join(&set.name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(".lock"))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        lock.lock()
            .map_err(|e| format!("locking {}: {e}", dir.display()))?;
        let headers_t = newest_header(&set.include);
        for src in &set.sources {
            let src = PathBuf::from(src);
            let obj = dir.join(object_name(&src));
            let cxx = is_cxx(&src);
            out.needs_cxx |= cxx;
            let args = compile_args(set, &embed_include, &src, &obj, cxx, debug, spec);
            let driver = if cxx {
                cc.cxx_command(config, spec)?
            } else {
                cc.cc_command(config, spec)?
            };
            let stamp_path = obj.with_extension("o.args");
            let stamp = format!(
                "{}\n{cc:?}\n{spec:?}\n{}\n{:?}\n{}",
                args.join("\n"),
                config.fingerprint(),
                driver.get_args().collect::<Vec<_>>(),
                crate::driver::rt_stamp::fingerprint(vec![PathBuf::from(driver.get_program())])
            );
            let fresh = object_fresh(&obj, &src, headers_t)
                && std::fs::read_to_string(&stamp_path).is_ok_and(|s| s == stamp);
            if !fresh {
                let mut cmd = if cxx {
                    cc.cxx_command(config, spec)?
                } else {
                    cc.cc_command(config, spec)?
                };
                cmd.args(&args);
                if let Err(e) = run_captured(&mut cmd, &format!("compiling {}", src.display())) {
                    let _ = std::fs::remove_file(&obj);
                    return Err(strip_warning_noise(&e));
                }
                std::fs::write(&stamp_path, &stamp)
                    .map_err(|e| format!("{}: {e}", stamp_path.display()))?;
            }
            out.objects.push(obj);
        }
        for fw in &set.frameworks {
            if spec.is_apple() {
                out.link_args.push("-framework".into());
                out.link_args.push(fw.clone());
            }
        }
        for lib in &set.libs {
            out.link_args.push(format!("-l{lib}"));
        }
        for e in &set.runtime_exports {
            if !out.runtime_exports.contains(e) {
                out.runtime_exports.push(e.clone());
            }
        }
    }
    if out.needs_cxx && !spec.is_msvc() {
        out.link_args.push("-lc++".into());
    }
    Ok(out)
}

/// `embed_include` holds the public `dream_embed.h`, after the set's own include dirs.
fn compile_args(
    set: &CSourceSet,
    embed_include: &Path,
    src: &Path,
    obj: &Path,
    cxx: bool,
    debug: bool,
    spec: &dream_abi::target::TargetSpec,
) -> Vec<String> {
    let mut args = vec![
        "-c".to_string(),
        if cxx { "-std=gnu++20" } else { "-std=gnu11" }.to_string(),
    ];
    if !spec.is_windows() {
        args.push("-fPIC".to_string());
    }
    if debug {
        args.extend(["-O0".to_string(), "-g".to_string()]);
    } else {
        args.push("-O2".to_string());
    }
    for inc in &set.include {
        args.push(format!("-I{inc}"));
    }
    args.push(format!("-I{}", embed_include.display()));
    for d in &set.defines {
        args.push(format!("-D{d}"));
    }
    args.extend(set.cflags.iter().cloned());
    args.push(src.display().to_string());
    args.push("-o".to_string());
    args.push(obj.display().to_string());
    args
}

/// A stable, collision-free object name: the source stem plus a hash of its full path.
fn object_name(src: &Path) -> String {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("src");
    format!(
        "{stem}-{:016x}.o",
        fnv1a(src.display().to_string().as_bytes())
    )
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

fn object_fresh(obj: &Path, src: &Path, headers: Option<SystemTime>) -> bool {
    let Some(obj_t) = mtime(obj) else {
        return false;
    };
    mtime(src).is_some_and(|t| t <= obj_t) && headers.is_none_or(|t| t <= obj_t)
}

fn newest_header(include: &[String]) -> Option<SystemTime> {
    let mut newest = None;
    for dir in include {
        walk_newest(Path::new(dir), &mut newest);
    }
    newest
}

fn walk_newest(dir: &Path, newest: &mut Option<SystemTime>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_newest(&p, newest);
        } else if let Some(t) = mtime(&p) {
            if newest.is_none_or(|n| t > n) {
                *newest = Some(t);
            }
        }
    }
}

/// Zig builds libc++ from source the first time, which floods stderr with nullability warnings
/// from its own headers; keep only the lines about the user's code.
fn strip_warning_noise(msg: &str) -> String {
    let mut out = Vec::new();
    let mut skipping = false;
    for line in msg.lines() {
        if line.contains("/lib/libcxx/") || line.contains("/lib/libcxxabi/") {
            skipping = true;
            continue;
        }
        if skipping && (line.starts_with(' ') || line.trim().is_empty()) {
            continue;
        }
        skipping = false;
        out.push(line);
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_names_are_stable_and_distinct() {
        let a = object_name(Path::new("/p/native/a/x.c"));
        let b = object_name(Path::new("/p/native/b/x.c"));
        assert_ne!(a, b);
        assert_eq!(a, object_name(Path::new("/p/native/a/x.c")));
        assert!(a.starts_with("x-") && a.ends_with(".o"));
    }

    #[test]
    fn cxx_units_use_gnu20_and_c_units_gnu11() {
        let set = CSourceSet {
            name: "s".into(),
            sources: vec![],
            include: vec!["/inc".into()],
            defines: vec!["A=1".into()],
            cflags: vec!["-Wall".into()],
            frameworks: vec![],
            libs: vec![],
            runtime_exports: vec![],
        };
        let c = compile_args(
            &set,
            Path::new("/rt"),
            Path::new("a.c"),
            Path::new("a.o"),
            false,
            false,
            &dream_abi::target::TargetSpec::host(),
        );
        assert!(c.contains(&"-std=gnu11".to_string()));
        assert_eq!(c.contains(&"-fPIC".to_string()), !cfg!(windows));
        assert!(c.contains(&"-I/inc".to_string()) && c.contains(&"-DA=1".to_string()));
        let cxx = compile_args(
            &set,
            Path::new("/rt"),
            Path::new("a.cpp"),
            Path::new("a.o"),
            true,
            true,
            &dream_abi::target::TargetSpec::host(),
        );
        assert!(cxx.contains(&"-std=gnu++20".to_string()) && cxx.contains(&"-g".to_string()));
    }

    #[test]
    fn drops_libcxx_build_noise() {
        let msg = "compiling x failed\n/z/lib/libcxx/include/string:1:1: warning: w\n   1 | x\n      |  ^\nkv.dream:12:5: error: bad\n";
        let cleaned = strip_warning_noise(msg);
        assert!(!cleaned.contains("libcxx"));
        assert!(cleaned.contains("kv.dream:12:5: error: bad"));
    }
}
