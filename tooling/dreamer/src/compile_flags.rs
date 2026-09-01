//! Flags forwarded to `dream` using the same tokens as the compiler CLI.

use anyhow::{bail, Result};
use std::process::Command;

/// `--release`, `-O`/`--optimize`, and `--wasm` (native C is the default).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileFlags {
    pub release: bool,
    /// `0`–`4`, `s`, or `z`. Bare `-O` is `"s"` (`-Os`).
    pub optimize: Option<String>,
    /// `true` = native C (default). `false` = wasm32 module (`--wasm`).
    pub native_c: bool,
}

impl Default for CompileFlags {
    fn default() -> Self {
        Self {
            release: false,
            optimize: None,
            native_c: true,
        }
    }
}

impl CompileFlags {
    pub fn from_cli(release: bool, optimize: Option<String>, wasm: bool) -> Result<Self> {
        if let Some(lvl) = optimize.as_deref() {
            if !matches!(lvl, "0" | "1" | "2" | "3" | "4" | "s" | "S" | "z" | "Z") {
                bail!("invalid optimization level '{lvl}' (expected one of: 0, 1, 2, 3, 4, s, z)");
            }
        }
        Ok(Self {
            release,
            optimize: optimize.map(|s| s.to_ascii_lowercase()),
            native_c: !wasm,
        })
    }

    /// Native pack: default `--release` (cc `-O3`). Explicit `-O` / `--release` match `dreamer run`.
    pub fn for_pack(release: bool, optimize: Option<String>, wasm: bool) -> Result<Self> {
        if wasm {
            bail!("pack produces a native executable; omit --wasm");
        }
        let release = release || optimize.is_none();
        Self::from_cli(release, optimize, false)
    }

    /// `target/release` vs `target/debug`, matching `dream`'s native output layout.
    pub fn native_artifact_subdir(&self) -> &'static str {
        if self.release {
            "release"
        } else {
            "debug"
        }
    }

    pub fn apply(&self, cmd: &mut Command) {
        if self.release {
            cmd.arg("--release");
        }
        if let Some(lvl) = &self.optimize {
            cmd.arg(format!("-O{lvl}"));
        }
        if !self.native_c {
            cmd.arg("--wasm");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_emits_dream_tokens() {
        let flags = CompileFlags::from_cli(true, Some("3".into()), false).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--release", "-O3"]);
    }

    #[test]
    fn apply_wasm_output() {
        let flags = CompileFlags::from_cli(false, None, true).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--wasm"]);
    }

    #[test]
    fn bare_optimize_is_size() {
        let flags = CompileFlags::from_cli(false, Some("s".into()), false).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["-Os"]);
    }

    #[test]
    fn pack_defaults_to_release() {
        let flags = CompileFlags::for_pack(false, None, false).unwrap();
        assert!(flags.release);
        assert!(flags.optimize.is_none());
        assert_eq!(flags.native_artifact_subdir(), "release");
    }

    #[test]
    fn pack_optimize_without_release_is_debug_like_run() {
        let flags = CompileFlags::for_pack(false, Some("2".into()), false).unwrap();
        assert!(!flags.release);
        assert_eq!(flags.optimize.as_deref(), Some("2"));
        assert_eq!(flags.native_artifact_subdir(), "debug");
    }

    #[test]
    fn pack_rejects_wasm() {
        assert!(CompileFlags::for_pack(false, None, true).is_err());
    }
}
