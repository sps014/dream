//! `.wasm` post-processing via Binaryen's `wasm-opt` toolchain executable, driven by `--release`
//! (default level [`OptLevel::RELEASE_DEFAULT`]) and/or an explicit `-O`/`--optimize` level. This
//! runs *after* the MIR pass pipeline already applied — an independent, coarser-grained
//! shrink/speed pass over the linked binary, not a replacement for the MIR passes.

use std::str::FromStr;

/// Optimization preset requested via `-O`/`--optimize=<LEVEL>`, mirroring `wasm-opt`'s own CLI
/// levels (`0`-`4`, `s`, `z`) so users can carry over familiar Binaryen knowledge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
    O4,
    /// `-Os`: optimize for size.
    Size,
    /// `-Oz`: optimize aggressively for size.
    SizeAggressive,
}

impl OptLevel {
    /// Default wasm-opt level for `--release` when no `-O`/`--optimize` is given. Speed (`-O3`)
    /// is the AOT LTO default; download-size builds pass `-Os` explicitly (`--web` / `-Os`).
    pub const RELEASE_DEFAULT: OptLevel = OptLevel::O3;
    /// wasm-opt level for downloadable `--web` modules when `--release` did not get an explicit `-O`.
    pub const WEB_RELEASE_DEFAULT: OptLevel = OptLevel::Size;
}

impl OptLevel {
    /// CLI token (`-O3`, `-Os`, …) so `dream` and `dreamer` pass the same flag.
    pub fn as_cli_flag(self) -> &'static str {
        match self {
            Self::O0 => "-O0",
            Self::O1 => "-O1",
            Self::O2 => "-O2",
            Self::O3 => "-O3",
            Self::O4 => "-O4",
            Self::Size => "-Os",
            Self::SizeAggressive => "-Oz",
        }
    }

    /// `--release` without `-O` is `-O3`; no flags is `-O0` (debug `cc`).
    pub fn from_cli(release: bool, explicit: Option<Self>) -> Self {
        explicit.unwrap_or(if release {
            Self::RELEASE_DEFAULT
        } else {
            Self::O0
        })
    }

    /// clang `-O` for wasm32 (no `-march=native`; wasm-opt applies the Binaryen level).
    pub fn wasm_clang_opt_flag(self) -> &'static str {
        match self {
            Self::O0 => "-O0",
            Self::O1 => "-O1",
            Self::O2 => "-O2",
            Self::O3 | Self::O4 => "-O3",
            Self::Size => "-Os",
            Self::SizeAggressive => "-Oz",
        }
    }

    pub fn native_rt_subdir(self) -> &'static str {
        match self {
            Self::O0 => "O0",
            Self::O1 => "O1",
            Self::O2 => "O2",
            Self::O3 | Self::O4 => "O3",
            Self::Size => "Os",
            Self::SizeAggressive => "Oz",
        }
    }
}

impl FromStr for OptLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `-O=Oz`, `-Oz`, `--optimize=Os` all appear on the CLI; keep the token after an optional
        // `-` / `O` prefix so users can spell Binaryen's flag or just the level letter.
        let t = s.trim().trim_start_matches('-');
        let t = t
            .strip_prefix('O')
            .or_else(|| t.strip_prefix('o'))
            .unwrap_or(t);
        match t {
            "0" => Ok(OptLevel::O0),
            "1" => Ok(OptLevel::O1),
            "2" => Ok(OptLevel::O2),
            "3" => Ok(OptLevel::O3),
            "4" => Ok(OptLevel::O4),
            "s" | "S" => Ok(OptLevel::Size),
            "z" | "Z" => Ok(OptLevel::SizeAggressive),
            other => Err(format!(
                "invalid optimization level '{other}' (expected one of: 0, 1, 2, 3, 4, s, z, Os, Oz)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_known_levels() {
        assert_eq!("0".parse::<OptLevel>(), Ok(OptLevel::O0));
        assert_eq!("1".parse::<OptLevel>(), Ok(OptLevel::O1));
        assert_eq!("2".parse::<OptLevel>(), Ok(OptLevel::O2));
        assert_eq!("3".parse::<OptLevel>(), Ok(OptLevel::O3));
        assert_eq!("4".parse::<OptLevel>(), Ok(OptLevel::O4));
        assert_eq!("s".parse::<OptLevel>(), Ok(OptLevel::Size));
        assert_eq!("z".parse::<OptLevel>(), Ok(OptLevel::SizeAggressive));
    }

    #[test]
    fn rejects_unknown_level() {
        assert!("bogus".parse::<OptLevel>().is_err());
    }

    #[test]
    fn release_default_is_speed() {
        assert_eq!(OptLevel::RELEASE_DEFAULT, OptLevel::O3);
        assert_eq!(OptLevel::WEB_RELEASE_DEFAULT, OptLevel::Size);
    }

    #[test]
    fn from_cli_matches_dream_tokens() {
        assert_eq!(OptLevel::from_cli(false, None), OptLevel::O0);
        assert_eq!(OptLevel::from_cli(true, None), OptLevel::O3);
        assert_eq!(
            OptLevel::from_cli(true, Some(OptLevel::Size)),
            OptLevel::Size
        );
        assert_eq!(OptLevel::O3.as_cli_flag(), "-O3");
        assert_eq!(OptLevel::Size.as_cli_flag(), "-Os");
        assert_eq!(OptLevel::O3.native_rt_subdir(), "O3");
        assert_eq!(OptLevel::O4.native_rt_subdir(), "O3");
        assert_eq!(OptLevel::O0.wasm_clang_opt_flag(), "-O0");
        assert_eq!(OptLevel::O3.wasm_clang_opt_flag(), "-O3");
        assert_eq!(OptLevel::Size.wasm_clang_opt_flag(), "-Os");
        assert_eq!(OptLevel::SizeAggressive.wasm_clang_opt_flag(), "-Oz");
    }
}
