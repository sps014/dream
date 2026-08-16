//! End-to-end coverage gate for the LLVM backend.
//!
//! Compiles every `tests/cases/*.dream` through the real driver and runs the native binary.

use dream::driver::compiler::{Compiler, Target};
use dream_abi::attributes::CompileTargets;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const XFAIL: &[(&str, &str)] = &[];

const WEB_COMPILE_CASES: &[&str] = &["state_map"];

fn compile_and_run_mir(dream_file: &Path) -> Result<String, String> {
    let out_path = dream_file.with_extension("mir.ll");
    let dream_str = dream_file.to_str().unwrap().to_string();
    let out_str = out_path.to_str().unwrap().to_string();

    let stem = dream_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut compiler = Compiler::new(Target::Native);
    if WEB_COMPILE_CASES.contains(&stem) {
        compiler = compiler.with_compile_targets(CompileTargets {
            native: true,
            node: false,
            web: true,
        });
    }
    compiler
        .compile(&dream_str, &out_str)
        .map_err(|e| format!("compile: {e:?}"))?;

    let bin = out_path.with_extension("out");
    let stdout = dream::execution::native_runner::execute_native_capturing(&bin)
        .map_err(|e| format!("execute: {e}"))?;
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_file(&bin);
    let _ = fs::remove_file(out_path.with_extension("abi.json"));
    let _ = fs::remove_file(dream_file.with_extension("mir.abi.json"));
    Ok(stdout)
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

    enum Outcome {
        Pass(String),
        Fail(String, String),
        UnexpectedPass(String),
    }

    let outcomes: Vec<Outcome> = entries
        .par_iter()
        .filter_map(|path| {
            let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
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

    if !failed.is_empty() || !unexpected_pass.is_empty() {
        let mut msg = String::new();
        for (stem, err) in &failed {
            msg.push_str(&format!("\n  FAIL {stem}: {err}"));
        }
        for stem in &unexpected_pass {
            msg.push_str(&format!("\n  unexpected pass {stem}"));
        }
        panic!("MIR e2e coverage failed:{}", msg);
    }
    assert!(
        !passed.is_empty(),
        "expected at least one MIR e2e fixture to run"
    );
}
