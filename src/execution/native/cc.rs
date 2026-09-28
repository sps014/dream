//! Resolve the system C compiler driver that links native binaries: env, `dreamer toolchain` Zig,
//! then PATH, and when none exists, install the Zig toolchain once (`DREAM_NO_AUTO_INSTALL=1`
//! opts out).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const MISSING_CC: &str =
    "no linker driver found for native builds; run `dreamer toolchain install cc`, \
     or set CC / DREAM_CC to a clang-compatible compiler";

#[derive(Debug, Clone)]
pub enum Cc {
    Program(PathBuf),
    Zig(PathBuf),
}

impl Cc {
    pub fn cc_command(&self) -> Command {
        match self {
            Self::Program(p) => Command::new(p),
            Self::Zig(p) => {
                let mut c = Command::new(p);
                c.arg("cc");
                c
            }
        }
    }
}

pub fn resolve_cc() -> Result<Cc, String> {
    if let Some(p) = env_program("DREAM_CC").or_else(|| env_program("CC")) {
        return Ok(classify_program(p));
    }
    if let Some(p) = env_program("DREAM_ZIG") {
        if p.is_file() {
            return Ok(Cc::Zig(p));
        }
    }
    if let Some(zig) = find_toolchain_zig() {
        return Ok(Cc::Zig(zig));
    }
    if let Some(p) = find_on_path("cc") {
        return Ok(Cc::Program(p));
    }
    if let Some(p) = find_on_path("clang") {
        return Ok(Cc::Program(p));
    }
    auto_install_zig().map(Cc::Zig)
}

/// Runs `dreamer toolchain install cc` (the dreamer beside this binary, else on PATH) once per
/// process, reporting its progress on stderr.
fn auto_install_zig() -> Result<PathBuf, String> {
    static INSTALLED: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            if std::env::var_os("DREAM_NO_AUTO_INSTALL").is_some_and(|v| !v.is_empty()) {
                return Err(MISSING_CC.into());
            }
            let name = if cfg!(windows) { "dreamer.exe" } else { "dreamer" };
            let beside = std::env::current_exe()
                .ok()
                .map(|e| std::fs::canonicalize(&e).unwrap_or(e))
                .and_then(|e| Some(e.parent()?.join(name)))
                .filter(|p| p.is_file());
            let dreamer = beside
                .or_else(|| find_on_path("dreamer"))
                .ok_or_else(|| MISSING_CC.to_string())?;
            eprintln!("no linker found; installing the Zig toolchain (set DREAM_NO_AUTO_INSTALL=1 to skip)");
            let status = Command::new(&dreamer)
                .args(["toolchain", "install", "cc"])
                .stdout(Stdio::from(std::io::stderr()))
                .status()
                .map_err(|e| format!("running {}: {e}", dreamer.display()))?;
            if !status.success() {
                return Err(format!("`dreamer toolchain install cc` failed; {MISSING_CC}"));
            }
            find_toolchain_zig().ok_or_else(|| MISSING_CC.to_string())
        })
        .clone()
}

/// A C compiler driver that links with the platform's own linker, for binaries the Zig toolchain
/// cannot link: `DREAM_CC`/`CC` unless they name zig, then `cc`/`clang` on PATH.
pub fn resolve_system_cc() -> Option<PathBuf> {
    env_program("DREAM_CC")
        .or_else(|| env_program("CC"))
        .filter(|p| matches!(classify_program(p.clone()), Cc::Program(_)))
        .or_else(|| find_on_path("cc"))
        .or_else(|| find_on_path("clang"))
}

fn classify_program(p: PathBuf) -> Cc {
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == "zig" {
        Cc::Zig(p)
    } else {
        Cc::Program(p)
    }
}

pub(super) fn env_program(key: &str) -> Option<PathBuf> {
    let v = std::env::var(key).ok()?;
    if v.is_empty() {
        return None;
    }
    let p = PathBuf::from(v);
    if p.is_file() {
        return Some(p);
    }
    find_on_path(p.to_str()?)
}

fn find_toolchain_zig() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DREAM_TOOLCHAINS") {
        if !dir.is_empty() {
            if let Some(p) = zig_in_dir(Path::new(&dir)) {
                return Some(p);
            }
        }
    }
    zig_in_dir(&toolchains_dir())
}

fn zig_in_dir(toolchains: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "zig.exe" } else { "zig" };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(toolchains) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.starts_with("zig-"))
            {
                dirs.push(p);
            }
        }
    }
    dirs.sort();
    dirs.into_iter()
        .rev()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

pub(crate) fn toolchains_dir() -> PathBuf {
    user_dream_dir().join("toolchains")
}

fn user_dream_dir() -> PathBuf {
    if let Ok(p) = std::env::var("DREAM_PREFIX") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(home) = std::env::var("DREAM_HOME") {
        if !home.is_empty() {
            let p = PathBuf::from(&home);
            if p.file_name().and_then(|s| s.to_str()) == Some("bin") {
                if let Some(parent) = p.parent() {
                    return parent.to_path_buf();
                }
            }
            let is_cargo_target = matches!(
                p.file_name().and_then(|s| s.to_str()),
                Some("debug" | "release")
            ) && p
                .parent()
                .and_then(|par| par.file_name())
                .and_then(|s| s.to_str())
                == Some("target");
            if !is_cargo_target {
                return p;
            }
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".dream")
}

pub fn native_rt_cache_root() -> PathBuf {
    if Path::new("Cargo.toml").is_file() && Path::new("target").is_dir() {
        return PathBuf::from("target/dream-native-rt");
    }
    user_dream_dir().join("cache").join("native-rt")
}

/// Root for compiled source-generator harnesses. Harness fingerprints already cover the compiler
/// internals that shape the emitted IR, so the artifact is per-toolchain, not per-project — caching
/// it per user keeps the ~10s cold build off every new project's first compile.
pub fn generator_cache_root() -> PathBuf {
    if Path::new("Cargo.toml").is_file() && Path::new("target").is_dir() {
        return PathBuf::from("target/generators");
    }
    user_dream_dir().join("cache").join("generators")
}

pub(super) fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(windows) && !name.ends_with(".exe") && !name.contains('/') {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(&exe_name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_zig_stem() {
        match classify_program(PathBuf::from("/opt/zig")) {
            Cc::Zig(p) => assert_eq!(p, PathBuf::from("/opt/zig")),
            Cc::Program(_) => panic!("expected zig"),
        }
        match classify_program(PathBuf::from("/usr/bin/cc")) {
            Cc::Program(p) => assert_eq!(p, PathBuf::from("/usr/bin/cc")),
            Cc::Zig(_) => panic!("expected cc"),
        }
    }
}
