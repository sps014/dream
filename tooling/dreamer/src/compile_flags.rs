//! Flags forwarded to `dream` using the same tokens as the compiler CLI.

use anyhow::{bail, Result};
use std::process::Command;

/// `--release`, `-O`/`--optimize`, and `--backend wasm` (native C is the default).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileFlags {
    pub release: bool,
    /// `0`–`4`, `s`, or `z`. Bare `-O` is `"s"` (`-Os`).
    pub optimize: Option<String>,
    /// `true` = native C (default). `false` = WAT (`--backend wasm`).
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
    pub fn from_cli(
        release: bool,
        optimize: Option<String>,
        backend: Option<String>,
    ) -> Result<Self> {
        let native_c = match backend.as_deref() {
            None | Some("c") => true,
            Some("wasm") => false,
            Some(other) => bail!("unknown --backend '{other}': expected wasm or c"),
        };
        if let Some(lvl) = optimize.as_deref() {
            if !matches!(lvl, "0" | "1" | "2" | "3" | "4" | "s" | "S" | "z" | "Z") {
                bail!("invalid optimization level '{lvl}' (expected one of: 0, 1, 2, 3, 4, s, z)");
            }
        }
        Ok(Self {
            release,
            optimize: optimize.map(|s| s.to_ascii_lowercase()),
            native_c,
        })
    }

    pub fn apply(&self, cmd: &mut Command) {
        if self.release {
            cmd.arg("--release");
        }
        if let Some(lvl) = &self.optimize {
            cmd.arg(format!("-O{lvl}"));
        }
        if !self.native_c {
            cmd.arg("--backend");
            cmd.arg("wasm");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_emits_dream_tokens() {
        let flags = CompileFlags::from_cli(true, Some("3".into()), None).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--release", "-O3"]);
    }

    #[test]
    fn apply_wasm_backend() {
        let flags = CompileFlags::from_cli(false, None, Some("wasm".into())).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--backend", "wasm"]);
    }

    #[test]
    fn bare_optimize_is_size() {
        let flags = CompileFlags::from_cli(false, Some("s".into()), None).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["-Os"]);
    }
}
