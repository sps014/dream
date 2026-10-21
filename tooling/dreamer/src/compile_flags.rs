//! Flags forwarded to `dream` using the same tokens as the compiler CLI.

use anyhow::{Result, bail};
use std::process::Command;

/// `--release`, `-O`/`--optimize`, and `--wasm` (native is the default).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileFlags {
    pub compile_profile: dream_abi::profile::CompileProfile,
    /// `0`–`4`, `s`, or `z`. Bare `-O` is `"s"` (`-Os`).
    pub optimize: Option<String>,
    /// `true` = native binary (default). `false` = wasm32 module (`--wasm`).
    pub native: bool,
    /// `--profile`: instrumented native build that records PGO profiles.
    pub profile: bool,
    /// `--use-profile[=<path>]`: `Some("")` merges the recorded `--profile` runs.
    pub use_profile: Option<String>,
    pub relocatable: bool,
}

impl Default for CompileFlags {
    fn default() -> Self {
        Self {
            compile_profile: dream_abi::profile::CompileProfile::Debug,
            optimize: None,
            native: true,
            profile: false,
            use_profile: None,
            relocatable: false,
        }
    }
}

impl CompileFlags {
    pub fn from_cli(release: bool, optimize: Option<String>, wasm: bool) -> Result<Self> {
        if let Some(lvl) = optimize.as_deref()
            && !matches!(lvl, "0" | "1" | "2" | "3" | "4" | "s" | "S" | "z" | "Z")
        {
            bail!("invalid optimization level '{lvl}' (expected one of: 0, 1, 2, 3, 4, s, z)");
        }
        if optimize.is_some() && !release {
            bail!("-O requires --release; debug builds always compile at -O0");
        }
        Ok(Self {
            compile_profile: dream_abi::profile::CompileProfile::from_release(release),
            optimize: optimize.map(|s| s.to_ascii_lowercase()),
            native: !wasm,
            profile: false,
            use_profile: None,
            relocatable: false,
        })
    }

    pub fn with_pgo(mut self, profile: bool, use_profile: Option<String>) -> Result<Self> {
        if (profile || use_profile.is_some()) && self.compile_profile.is_debug() {
            bail!("--profile / --use-profile require --release");
        }
        self.profile = profile;
        self.use_profile = use_profile;
        Ok(self)
    }

    /// Native pack: always a release build (cc `-O3` unless `-O` picks another level).
    pub fn for_pack(optimize: Option<String>, wasm: bool) -> Result<Self> {
        if wasm {
            bail!("pack produces a native executable; omit --wasm");
        }
        let mut flags = Self::from_cli(true, optimize, false)?;
        flags.relocatable = true;
        Ok(flags)
    }

    /// `target/release` vs `target/debug`, matching `dream`'s native output layout.
    pub fn native_artifact_subdir(&self) -> &'static str {
        if !self.compile_profile.is_debug() {
            "release"
        } else {
            "debug"
        }
    }

    pub fn apply(&self, cmd: &mut Command) {
        if self.relocatable {
            cmd.arg("--relocatable");
        }
        if !self.compile_profile.is_debug() {
            cmd.arg("--release");
        }
        if let Some(lvl) = &self.optimize {
            cmd.arg(format!("-O{lvl}"));
        }
        if !self.native {
            cmd.arg("--wasm");
        }
        if self.profile {
            cmd.arg("--profile");
        }
        match self.use_profile.as_deref() {
            Some("") => {
                cmd.arg("--use-profile");
            }
            Some(p) => {
                cmd.arg(format!("--use-profile={p}"));
            }
            None => {}
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
        let flags = CompileFlags::from_cli(true, Some("s".into()), false).unwrap();
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--release", "-Os"]);
    }

    #[test]
    fn optimize_requires_release() {
        let error = CompileFlags::from_cli(false, Some("3".into()), false).unwrap_err();
        assert!(error.to_string().contains("--release"));
    }

    #[test]
    fn apply_forwards_pgo_flags() {
        let args = |flags: CompileFlags| -> Vec<String> {
            let mut cmd = Command::new("dream");
            flags.apply(&mut cmd);
            cmd.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let base = CompileFlags::from_cli(true, None, false).unwrap();
        assert_eq!(
            args(base.clone().with_pgo(true, None).unwrap()),
            ["--release", "--profile"]
        );
        assert_eq!(
            args(base.clone().with_pgo(false, Some(String::new())).unwrap()),
            ["--release", "--use-profile"]
        );
        assert_eq!(
            args(base.with_pgo(false, Some("run.profdata".into())).unwrap()),
            ["--release", "--use-profile=run.profdata"]
        );
        let debug = CompileFlags::from_cli(false, None, false).unwrap();
        assert!(debug.with_pgo(true, None).is_err());
    }

    #[test]
    fn pack_defaults_to_release() {
        let flags = CompileFlags::for_pack(None, false).unwrap();
        assert!(!flags.compile_profile.is_debug());
        assert!(flags.optimize.is_none());
        assert_eq!(flags.native_artifact_subdir(), "release");
        assert!(flags.relocatable);
        let mut cmd = Command::new("dream");
        flags.apply(&mut cmd);
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            ["--relocatable", "--release"]
        );
    }

    #[test]
    fn pack_optimize_stays_release() {
        let flags = CompileFlags::for_pack(Some("2".into()), false).unwrap();
        assert!(!flags.compile_profile.is_debug());
        assert_eq!(flags.optimize.as_deref(), Some("2"));
        assert_eq!(flags.native_artifact_subdir(), "release");
    }

    #[test]
    fn pack_rejects_wasm() {
        assert!(CompileFlags::for_pack(None, true).is_err());
    }
}
