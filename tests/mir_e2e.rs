//! End-to-end coverage gate for the MIR native-C backend.
//!
//! Compiles every `tests/cases/*.dream` through the **real driver** (prelude, json-derive, multi-file
//! resolution, analysis, HIR → MIR → C), runs the `.bin`, and compares to `.expected`.
//!
//! The assertion is a ratchet: every case **not** in `XFAIL` must pass, and `XFAIL` is currently
//! empty (the backend covers the whole test corpus). Any regression that breaks a previously-passing
//! case fails the suite.

use dream::driver::compiler::{Compiler, Target};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::compile_and_capture;
use dream_abi::attributes::CompileTargets;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

// Every case in `tests/cases` now compiles and runs through the MIR backend, so `XFAIL` is empty.
// Keep it (rather than deleting the machinery) so a future regression re-adds an entry here with a
// reason instead of silently flipping the ratchet.
const XFAIL: &[(&str, &str)] = &[];

const WEB_COMPILE_CASES: &[&str] = &["state_map"];

/// Compiles one case through native C and runs it.
fn compile_and_run_mir(dream_file: &Path) -> Result<String, String> {
    let dest_dir = Path::new("target").join("mir-e2e-native");
    fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir: {e}"))?;
    let stem = dream_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("case");
    let c_path = dest_dir.join(format!("{stem}.c"));
    let dream_str = dream_file.to_str().unwrap().to_string();
    let c_str = c_path.to_string_lossy().into_owned();

    let mut compiler = Compiler::new(Target::NativeC);
    if WEB_COMPILE_CASES.contains(&stem) {
        compiler = compiler.with_compile_targets(CompileTargets {
            native: true,
            node: false,
            web: true,
        });
    }
    compiler
        .compile(&dream_str, &c_str)
        .map_err(|e| format!("compile: {e:?}"))?;

    let out = compile_and_capture(&c_str, OptLevel::O0).map_err(|e| format!("run: {e}"))?;
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(c_path.with_extension("o"));
    let _ = fs::remove_file(c_path.with_extension("bin"));
    let _ = fs::remove_file(c_path.with_extension("abi.json"));
    Ok(out)
}
#[test]
#[ignore = "duplicates run_all_e2e_cases; cargo test --workspace -- --ignored"]
fn mir_backend_e2e_coverage() {
    let cases_dir = Path::new("tests/cases");
    if !cases_dir.exists() {
        return;
    }
    let xfail: BTreeSet<&str> = XFAIL.iter().map(|(name, _)| *name).collect();

    let mut entries: Vec<_> = fs::read_dir(cases_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("dream"))
        .collect();
    entries.sort();

    // One classified outcome per case; `None` for cases that are not backend-coverage fixtures
    // (compile-error cases or those lacking a golden `.expected`). Runs in parallel across cores.
    enum Outcome {
        Pass(String),
        Fail(String, String),
        UnexpectedPass(String),
    }

    let outcomes: Vec<Outcome> = entries
        .par_iter()
        .filter_map(|path| {
            let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
            // Cases that are supposed to fail compilation are not backend-coverage cases.
            if path.with_extension("expected_error").exists() {
                return None;
            }
            let expected = fs::read_to_string(path.with_extension("expected")).ok()?;
            let is_xfail = xfail.contains(stem.as_str());
            Some(match compile_and_run_mir(path) {
                Ok(actual) if actual.trim() == expected.trim() => {
                    if is_xfail {
                        Outcome::UnexpectedPass(stem)
                    } else {
                        Outcome::Pass(stem)
                    }
                }
                Ok(_) if is_xfail => return None,
                Ok(actual) => {
                    Outcome::Fail(stem, format!("output mismatch: got {:?}", actual.trim()))
                }
                Err(_) if is_xfail => return None,
                Err(e) => Outcome::Fail(stem, e),
            })
        })
        .collect();

    let mut passed: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut unexpected_pass: Vec<String> = Vec::new();
    for outcome in outcomes {
        match outcome {
            Outcome::Pass(stem) => passed.push(stem),
            Outcome::Fail(stem, msg) => failed.push((stem, msg)),
            Outcome::UnexpectedPass(stem) => unexpected_pass.push(stem),
        }
    }
    passed.sort();
    failed.sort();
    unexpected_pass.sort();

    eprintln!(
        "\nMIR backend e2e coverage: {} passing, {} xfail, {} unexpectedly failing",
        passed.len(),
        xfail.len(),
        failed.len()
    );
    eprintln!("passing: {passed:?}");

    if !unexpected_pass.is_empty() {
        eprintln!("\nThese XFAIL cases now PASS — remove them from XFAIL:\n  {unexpected_pass:?}");
    }
    if !failed.is_empty() {
        let detail: String = failed
            .iter()
            .map(|(n, e)| format!("  {n}: {e}"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{} case(s) not in XFAIL failed through the MIR backend:\n{detail}",
            failed.len()
        );
    }
    assert!(
        unexpected_pass.is_empty(),
        "XFAIL is stale (see message above)"
    );
}
