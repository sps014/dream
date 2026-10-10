//! Profile-guided optimization through LLVM's IR-level instrumentation profiles.
//!
//! `--profile` links an instrumented binary that records one `.profraw` per run into
//! `<stem>.pgo/` next to it; `--use-profile` merges those with the pinned `llvm-profdata` and
//! rebuilds with the profile. The emitted `.ll` is identical in both modes, so profile hashes
//! match and codegen stays deterministic.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Pgo {
    #[default]
    Off,
    /// Build an instrumented binary whose runs record profiles into `<stem>.pgo/`.
    Generate,
    /// Optimize with a profile: an explicit `.profdata`, a `.profraw`, or a directory of
    /// `.profraw` files; `None` merges the `--profile` runs recorded next to the binary.
    Use(Option<PathBuf>),
}

/// The PGO mode as the `opt -pgo-kind` pipeline and its profile file (the raw
/// profile pattern when instrumenting, the merged `.profdata` when optimizing). `profdata` finds
/// the pinned toolchain's `llvm-profdata`, whose format matches the pinned instrumentation; only
/// a merge asks for it.
pub(crate) fn llvm_pgo(
    pgo: &Pgo,
    bin: &Path,
    profdata: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<Option<(&'static str, PathBuf)>, String> {
    match pgo {
        Pgo::Off => Ok(None),
        Pgo::Generate => Ok(Some((
            "pgo-instr-gen-pipeline",
            raw_dir(bin).join("default_%m.profraw"),
        ))),
        Pgo::Use(path) => {
            let merged = match profile_input(path.as_deref(), bin)? {
                ProfileInput::Merged(p) => p,
                ProfileInput::Raw(raws) => merge_with(&raws, bin, &profdata()?, path.is_none())?,
            };
            Ok(Some(("pgo-instr-use-pipeline", merged)))
        }
    }
}

/// Removes profiles recorded by a previous instrumented build so a new one never merges counters
/// from different code.
pub(crate) fn clear_raw_profiles(bin: &Path) {
    for raw in raw_profiles(&raw_dir(bin)) {
        let _ = std::fs::remove_file(raw);
    }
}

fn out_dir(bin: &Path) -> PathBuf {
    let dir = bin.parent().unwrap_or_else(|| Path::new("."));
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
}

fn stem(bin: &Path) -> String {
    bin.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dream")
        .to_string()
}

fn raw_dir(bin: &Path) -> PathBuf {
    out_dir(bin).join(format!("{}.pgo", stem(bin)))
}

fn raw_profiles(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "profraw"))
        .collect();
    out.sort();
    out
}

enum ProfileInput {
    Merged(PathBuf),
    Raw(Vec<PathBuf>),
}

fn profile_input(path: Option<&Path>, bin: &Path) -> Result<ProfileInput, String> {
    let raws = match path {
        Some(p) if p.extension().is_some_and(|e| e == "profdata") => {
            if !p.is_file() {
                return Err(format!("profile {} does not exist", p.display()));
            }
            return Ok(ProfileInput::Merged(p.to_path_buf()));
        }
        Some(p) if p.is_dir() => raw_profiles(p),
        Some(p) if p.is_file() => vec![p.to_path_buf()],
        Some(p) => return Err(format!("profile {} does not exist", p.display())),
        None => raw_profiles(&raw_dir(bin)),
    };
    if raws.is_empty() {
        return Err(format!(
            "no .profraw profiles for {}; build and run it with --profile first",
            bin.display()
        ));
    }
    Ok(ProfileInput::Raw(raws))
}

/// `reuse`: keep an existing merge that is newer than every input (the default `--profile` runs
/// only; an explicit input may be a different, older set).
fn merge_with(raws: &[PathBuf], bin: &Path, tool: &Path, reuse: bool) -> Result<PathBuf, String> {
    let out = out_dir(bin).join(format!("{}.profdata", stem(bin)));
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if let Some(merged) = modified(&out).filter(|_| reuse)
        && raws
            .iter()
            .all(|r| modified(r).is_some_and(|t| t <= merged))
    {
        return Ok(out);
    }
    let mut cmd = Command::new(tool);
    cmd.arg("merge").arg("-o").arg(&out).args(raws);
    let res = cmd
        .output()
        .map_err(|e| format!("could not run {}: {e}", tool.display()))?;
    if !res.status.success() {
        return Err(format!(
            "llvm-profdata merge failed\n{}",
            String::from_utf8_lossy(&res.stderr).trim()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dream-pgo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn use_without_recorded_runs_asks_for_profile() {
        let dir = scratch("empty");
        let err = match profile_input(None, &dir.join("app.bin")) {
            Err(e) => e,
            Ok(_) => panic!("expected an error"),
        };
        assert!(err.contains("--profile"), "{}", err);
    }

    #[test]
    fn recorded_runs_are_collected_sorted_from_the_pgo_dir() {
        let dir = scratch("runs");
        let bin = dir.join("app.bin");
        let pgo = raw_dir(&bin);
        std::fs::create_dir_all(&pgo).unwrap();
        for name in ["b.profraw", "a.profraw", "notes.txt"] {
            std::fs::write(pgo.join(name), b"x").unwrap();
        }
        match profile_input(None, &bin) {
            Ok(ProfileInput::Raw(raws)) => {
                let names: Vec<_> = raws
                    .iter()
                    .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                    .collect();
                assert_eq!(names, ["a.profraw", "b.profraw"]);
            }
            _ => panic!("expected raw profiles"),
        }
        clear_raw_profiles(&bin);
        assert!(raw_profiles(&pgo).is_empty());
    }

    #[test]
    fn missing_explicit_profdata_is_an_error() {
        let dir = scratch("missing");
        assert!(profile_input(Some(&dir.join("x.profdata")), &dir.join("app.bin")).is_err());
    }
}
