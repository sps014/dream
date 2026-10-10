//! Locate `@c("lib", …)` shared libraries and emit `cc` link flags.
//!
//! Search order: `native/` next to the artifact / source, CWD, then Homebrew/system dirs.
//! Native C links with `-L` / `-l` / `-rpath`.

use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct AbiFile {
    #[serde(default)]
    c_libs: Vec<String>,
}

/// Walks an artifact path's parent chain so `sample/foo/target/release/x.c` still finds
/// `sample/foo/native/`.
pub fn search_roots_for_artifact(
    config: &crate::driver::toolchain::ToolchainConfig,
    artifact: &Path,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut cur = artifact.parent().map(|p| p.to_path_buf());
    while let Some(dir) = cur {
        roots.push(dir.clone());
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    {
        let cwd = config.cwd.clone();
        if !roots.iter().any(|r| r == &cwd) {
            roots.push(cwd);
        }
    }
    roots
}

pub fn read_c_libs_from_abi(abi_path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(abi_path) else {
        return Vec::new();
    };
    serde_json::from_str::<AbiFile>(&text)
        .map(|a| a.c_libs)
        .unwrap_or_default()
}

pub fn library_file_names(lib_name: &str, spec: &dream_abi::target::TargetSpec) -> Vec<String> {
    if spec.is_windows() {
        vec![
            format!("{lib_name}.lib"),
            format!("lib{lib_name}.dll.a"),
            format!("{lib_name}.dll"),
            format!("lib{lib_name}.a"),
        ]
    } else if spec.is_apple() {
        vec![format!("lib{lib_name}.dylib"), format!("lib{lib_name}.a")]
    } else {
        vec![format!("lib{lib_name}.so"), format!("lib{lib_name}.a")]
    }
}

pub fn system_library_dirs(
    config: &crate::driver::toolchain::ToolchainConfig,
    spec: &dream_abi::target::TargetSpec,
) -> Vec<PathBuf> {
    let mut dirs = vec![config.targets.join(spec.triple.to_string()).join("lib")];
    if let Some(root) = &config.sysroot {
        dirs.extend([root.join("lib"), root.join("usr/lib")]);
    }
    if spec.can_link_on_host() {
        if spec.is_windows() {
            #[cfg(windows)]
            if let Some(win) = &config.windir {
                dirs.push(PathBuf::from(win).join("System32"));
            }
        } else {
            dirs.extend(["/usr/local/lib", "/usr/lib", "/lib"].map(PathBuf::from));
            if spec.is_apple() {
                dirs.extend(["/opt/homebrew/lib", "/opt/local/lib"].map(PathBuf::from));
            } else {
                dirs.push(PathBuf::from(format!(
                    "/usr/lib/{}-linux-gnu",
                    spec.triple.architecture
                )));
            }
        }
    }
    dirs
}

/// Resolves `lib_name` to a filesystem path. Does not probe the OS loader (that's the caller's
/// last resort when `dlopen`ing).
pub fn find_library_path(
    config: &crate::driver::toolchain::ToolchainConfig,
    lib_name: &str,
    search_roots: &[PathBuf],
    spec: &dream_abi::target::TargetSpec,
) -> Option<PathBuf> {
    let file_names = library_file_names(lib_name, spec);
    for root in search_roots {
        for name in &file_names {
            let native = root.join("native").join(name);
            if native.exists() {
                return Some(native);
            }
            let direct = root.join(name);
            if direct.exists() {
                return Some(direct);
            }
        }
    }
    for name in &file_names {
        let path = PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
    }
    for dir in system_library_dirs(config, spec) {
        for name in &file_names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// `-L` / `-l` / `-rpath` flags for each `@c` library. Always emits `-l<name>` so the
/// compiler's default search (macOS SDK `libsqlite3`, `LIBRARY_PATH`, …) still applies when
/// the dylib is not in a well-known directory.
pub fn cc_link_flags(
    config: &crate::driver::toolchain::ToolchainConfig,
    libs: &[String],
    search_roots: &[PathBuf],
    spec: &dream_abi::target::TargetSpec,
) -> Vec<String> {
    let mut flags = Vec::new();
    let mut rpaths = BTreeSet::new();
    for lib in libs {
        // The MSVC CRT supplies both C and math symbols; there are no c.lib/m.lib archives.
        if spec.is_msvc() && matches!(lib.as_str(), "c" | "m") {
            continue;
        }
        if let Some(path) = find_library_path(config, lib, search_roots, spec)
            && let Some(dir) = path.parent()
        {
            flags.push(format!("-L{}", dir.display()));
            rpaths.insert(dir.to_path_buf());
        }
        flags.push(format!("-l{lib}"));
    }
    if !spec.is_windows() {
        for dir in rpaths {
            flags.push(format!("-Wl,-rpath,{}", dir.display()));
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_crt_libraries() {
        let config = crate::driver::toolchain::ToolchainConfig::default();
        let flags = cc_link_flags(
            &config,
            &["c".into(), "m".into()],
            &[],
            &dream_abi::target::TargetSpec::host(),
        );
        if cfg!(all(windows, target_env = "msvc")) {
            assert!(flags.is_empty());
        } else {
            assert!(flags.iter().any(|arg| arg == "-lc"));
            assert!(flags.iter().any(|arg| arg == "-lm"));
        }
    }
}
