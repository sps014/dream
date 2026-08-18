//! Resolve a C compiler for the native-C backend: env, `dreamer toolchain` Zig, then PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

const MISSING_CC: &str =
    "no C compiler found for --backend c; run `dreamer toolchain install cc`, \
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

    pub fn ar_command(&self) -> Command {
        match self {
            Self::Zig(p) => {
                let mut c = Command::new(p);
                c.arg("ar");
                c
            }
            Self::Program(_) => {
                if let Ok(ar) = std::env::var("DREAM_AR").or_else(|_| std::env::var("AR")) {
                    if !ar.is_empty() {
                        return Command::new(ar);
                    }
                }
                Command::new("ar")
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
    Err(MISSING_CC.into())
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

fn env_program(key: &str) -> Option<PathBuf> {
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

fn toolchains_dir() -> PathBuf {
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

fn find_on_path(name: &str) -> Option<PathBuf> {
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
