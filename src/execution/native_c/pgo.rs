//! Profile-guided optimization through clang's IR-level instrumentation profiles.
//!
//! `--profile` links an instrumented binary that records one `.profraw` per run into
//! `<stem>.pgo/` next to it; `--use-profile` merges those with `llvm-profdata` and rebuilds with
//! `-fprofile-use`. The emitted `.c` is identical in both modes, so profile hashes match and
//! codegen stays deterministic. IR-level instrumentation (`-fprofile-generate`) measured clearly
//! ahead of front-end instrumentation (`-fprofile-instr-generate`) on the microbench suite, which
//! regressed most kernels. Zig's `cc` accepts the flags but links no profile runtime (no `.profraw`
//! is ever written), so a capable compiler is detected by actually producing a profile.

use super::cc::{env_program, find_on_path, Cc};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

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

/// The compiler and extra cc/link flags one PGO mode needs.
pub(super) struct PgoBuild {
    pub cc: Cc,
    pub flags: Vec<String>,
    /// Profile input whose modification must invalidate a cached binary.
    pub input: Option<PathBuf>,
}

const NO_PGO_CC: &str = "PGO needs clang with its profile runtime (-fprofile-generate); \
     zig cc links none. Install clang, or set DREAM_PGO_CC to a clang binary";

pub(super) fn prepare(pgo: &Pgo, default_cc: Cc, bin: &Path) -> Result<PgoBuild, String> {
    match pgo {
        Pgo::Off => Ok(PgoBuild {
            cc: default_cc,
            flags: Vec::new(),
            input: None,
        }),
        Pgo::Generate => Ok(PgoBuild {
            cc: pgo_cc(default_cc)?,
            flags: vec![format!("-fprofile-generate={}", raw_dir(bin).display())],
            input: None,
        }),
        Pgo::Use(path) => {
            let input = profile_input(path.as_deref(), bin)?;
            let cc = pgo_cc(default_cc)?;
            let profdata = match input {
                ProfileInput::Merged(p) => p,
                ProfileInput::Raw(raws) => merge(&raws, bin, &cc, path.is_none())?,
            };
            Ok(PgoBuild {
                cc,
                flags: vec![format!("-fprofile-use={}", profdata.display())],
                input: Some(profdata),
            })
        }
    }
}

/// Removes profiles recorded by a previous instrumented build so a new one never merges counters
/// from different code.
pub(super) fn clear_raw_profiles(bin: &Path) {
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
fn merge(raws: &[PathBuf], bin: &Path, cc: &Cc, reuse: bool) -> Result<PathBuf, String> {
    let out = out_dir(bin).join(format!("{}.profdata", stem(bin)));
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if let Some(merged) = modified(&out).filter(|_| reuse) {
        if raws
            .iter()
            .all(|r| modified(r).is_some_and(|t| t <= merged))
        {
            return Ok(out);
        }
    }
    let tool = llvm_profdata(cc)?;
    let mut cmd = Command::new(&tool);
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

fn pgo_cc(default_cc: Cc) -> Result<Cc, String> {
    static CAPABLE: OnceLock<Result<Cc, String>> = OnceLock::new();
    CAPABLE
        .get_or_init(|| {
            let mut candidates = Vec::new();
            if let Some(p) = env_program("DREAM_PGO_CC") {
                candidates.push(Cc::Program(p));
            } else {
                candidates.push(default_cc);
                candidates.extend(find_on_path("clang").map(Cc::Program));
            }
            candidates
                .into_iter()
                .find(writes_profile)
                .ok_or_else(|| NO_PGO_CC.to_string())
        })
        .clone()
}

/// Builds and runs a one-line program with `-fprofile-generate`, answering whether a
/// `.profraw` appeared.
fn writes_profile(cc: &Cc) -> bool {
    let dir = std::env::temp_dir().join(format!("dream-pgo-probe-{}", std::process::id()));
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let src = dir.join("probe.c");
    let exe = dir.join("probe.bin");
    let raw = dir.join("probe.profraw");
    let ok = std::fs::write(&src, "int main(void) { return 0; }\n").is_ok()
        && cc
            .cc_command()
            .args(["-O1", "-fprofile-generate", "-fno-sanitize=undefined"])
            .arg(&src)
            .arg("-o")
            .arg(&exe)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        && Command::new(&exe)
            .env("LLVM_PROFILE_FILE", &raw)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        && raw.is_file();
    let _ = std::fs::remove_dir_all(&dir);
    ok
}

/// `llvm-profdata` matching `cc`: an explicit override, the tool beside the compiler, the Xcode
/// toolchain's, then `PATH`.
fn llvm_profdata(cc: &Cc) -> Result<PathBuf, String> {
    if let Some(p) = env_program("DREAM_LLVM_PROFDATA").or_else(|| env_program("LLVM_PROFDATA")) {
        return Ok(p);
    }
    if let Cc::Program(p) = cc {
        let real = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        if let Some(sib) = real.parent().map(|d| d.join("llvm-profdata")) {
            if sib.is_file() {
                return Ok(sib);
            }
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("xcrun").args(["-f", "llvm-profdata"]).output() {
            let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if out.status.success() && p.is_file() {
                return Ok(p);
            }
        }
    }
    find_on_path("llvm-profdata").ok_or_else(|| {
        "llvm-profdata not found (needed to merge .profraw profiles); install the LLVM tools \
         matching your clang, or set DREAM_LLVM_PROFDATA"
            .to_string()
    })
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
