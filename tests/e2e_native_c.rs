//! Native C golden lane: same `tests/cases` fixtures as Wasmtime, via `Target::NativeC` + `cc`.

use dream::driver::compiler::{Compiler, Target};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::compile_and_capture;
use pretty_assertions::assert_eq;
use rayon::prelude::*;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

const SMOKE_CASES: &[&str] = &["arithmetic", "concat", "control_flow"];

fn collect_case_paths() -> Vec<PathBuf> {
    let cases_dir = Path::new("tests/cases");
    if !cases_dir.exists() {
        return Vec::new();
    }
    fs::read_dir(cases_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("dream"))
        .collect()
}

fn run_native_case(dream_file: &Path, release: bool) {
    let expected_file = dream_file.with_extension("expected");
    let expected_error_file = dream_file.with_extension("expected_error");
    let expected_trap_file = dream_file.with_extension("expected_trap");
    let stem = dream_file.file_stem().and_then(|s| s.to_str()).unwrap();
    let dest_dir = Path::new("target").join(if release {
        "e2e-native-c-release"
    } else {
        "e2e-native-c"
    });
    fs::create_dir_all(&dest_dir).unwrap();
    let c_path = dest_dir.join(format!("{stem}.c"));
    let compiler = Compiler::new(Target::NativeC).with_release(release);
    let src = dream_file.to_str().unwrap().to_string();
    let dest = c_path.to_str().unwrap().to_string();
    let compile_result = compiler.compile(&src, &dest);

    if expected_error_file.exists() {
        assert!(
            compile_result.is_err(),
            "Expected compilation to fail for {:?}",
            dream_file
        );
        let _ = fs::remove_file(&c_path);
        let _ = fs::remove_file(c_path.with_extension("o"));
        return;
    }
    compile_result.unwrap_or_else(|e| panic!("compile failed for {:?}: {e}", dream_file));

    let expects_trap = expected_trap_file.exists();
    let expected_output = if expects_trap {
        fs::read_to_string(&expected_trap_file).unwrap_or_default()
    } else if expected_file.exists() {
        fs::read_to_string(&expected_file).unwrap_or_default()
    } else {
        String::new()
    };

    let run = compile_and_capture(
        c_path.to_str().unwrap(),
        if release { OptLevel::O3 } else { OptLevel::O0 },
    );
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(c_path.with_extension("o"));
    let _ = fs::remove_file(c_path.with_extension("bin"));

    if expects_trap {
        assert!(run.is_err(), "expected trap for {:?}", dream_file);
        return;
    }
    let actual = run.unwrap_or_else(|e| panic!("run failed for {:?}: {e}", dream_file));
    assert_eq!(
        actual.trim(),
        expected_output.trim(),
        "Output mismatch for {:?}",
        dream_file
    );
}

fn run_corpus(release: bool, only: Option<&[&str]>) {
    let mut paths = collect_case_paths();
    if let Some(stems) = only {
        paths.retain(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| stems.contains(&s))
        });
    }
    let failures: Vec<String> = paths
        .par_iter()
        .filter_map(|path| {
            match catch_unwind(AssertUnwindSafe(|| run_native_case(path, release))) {
                Ok(()) => None,
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown panic".to_string());
                    Some(format!("{:?}: {}", path, msg))
                }
            }
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} native-C e2e case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn run_smoke_native_c_cases() {
    run_corpus(false, Some(SMOKE_CASES));
}

#[test]
#[ignore = "full native C golden corpus; cargo test --workspace -- --ignored"]
fn run_all_native_c_cases() {
    run_corpus(false, None);
}

#[test]
#[ignore = "full native C release corpus; cargo test --workspace -- --ignored"]
fn run_all_native_c_cases_release() {
    run_corpus(true, None);
}
