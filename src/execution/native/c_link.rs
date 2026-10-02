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
pub fn search_roots_for_artifact(artifact: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut cur = artifact.parent().map(|p| p.to_path_buf());
    while let Some(dir) = cur {
        roots.push(dir.clone());
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
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

pub fn library_file_names(lib_name: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        vec![format!("{lib_name}.dll"), format!("lib{lib_name}.dll")]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            format!("lib{lib_name}.dylib"),
            format!("lib{lib_name}.a"),
            format!("{lib_name}.dylib"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            format!("lib{lib_name}.so"),
            format!("lib{lib_name}.a"),
            format!("{lib_name}.so"),
        ]
    }
}

pub fn system_library_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/lib"),
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/opt/local/lib"),
            PathBuf::from("/usr/lib"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/usr/lib/x86_64-linux-gnu"),
            PathBuf::from("/usr/lib/aarch64-linux-gnu"),
            PathBuf::from("/usr/lib64"),
            PathBuf::from("/usr/lib"),
            PathBuf::from("/lib"),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let mut dirs = Vec::new();
        if let Ok(win) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(win).join("System32"));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\System32"));
        }
        dirs
    }
}

/// Resolves `lib_name` to a filesystem path. Does not probe the OS loader (that's the caller's
/// last resort when `dlopen`ing).
pub fn find_library_path(lib_name: &str, search_roots: &[PathBuf]) -> Option<PathBuf> {
    let file_names = library_file_names(lib_name);
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
    for dir in system_library_dirs() {
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
pub fn cc_link_flags(libs: &[String], search_roots: &[PathBuf]) -> Vec<String> {
    let mut flags = Vec::new();
    let mut rpaths = BTreeSet::new();
    for lib in libs {
        if let Some(path) = find_library_path(lib, search_roots) {
            if let Some(dir) = path.parent() {
                flags.push(format!("-L{}", dir.display()));
                rpaths.insert(dir.to_path_buf());
            }
        }
        flags.push(format!("-l{lib}"));
    }
    if !cfg!(target_os = "windows") {
        for dir in rpaths {
            flags.push(format!("-Wl,-rpath,{}", dir.display()));
        }
    }
    flags
}
