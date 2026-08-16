use dream::driver::compiler::{Compiler, Target};
use pretty_assertions::assert_eq;
use rayon::prelude::*;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;

fn clang_can_emit_wasm32() -> bool {
    let clang = std::env::var("DREAM_CLANG").unwrap_or_else(|_| "clang".to_string());
    let out = Command::new(&clang)
        .args(["-print-targets"])
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.contains("wasm32")
                && Command::new("wasm-ld")
                    .arg("-version")
                    .output()
                    .map(|v| v.status.success())
                    .unwrap_or(false)
        }
        Err(_) => false,
    }
}

/// Cases that read `Debug.live_objects()` / `total_allocations()`. Those probes only return real
/// counts when the allocator is instrumented (the default debug build), so they only produce
/// correct output in debug. The release suite runs these in debug (bypassing `--release`) rather
/// than release, so their full output stays asserted.
const DEBUG_ONLY_CASES: &[&str] = &[
    "struct_rc",
    "memory_advanced",
    "struct_container_rc",
    "value_union_option",
    "gc_complete",
    "closure_env_reclaim",
    "closure_env_escape",
    "async_rc_alias",
    "async_rc_return",
    "async_rc_reassign",
    "ui_render_tree",
];

fn run_test_case(dream_file: &Path, release: bool, wat_ext: &str) {
    let expected_file = dream_file.with_extension("expected");
    let expected_error_file = dream_file.with_extension("expected_error");
    let expected_trap_file = dream_file.with_extension("expected_trap");

    let compiler = Compiler::new(Target::Native).with_release(release);
    let wat_path = dream_file.with_extension(wat_ext);

    let dream_file_str = dream_file.to_str().unwrap().to_string();
    let wat_path_str = wat_path.to_str().unwrap().to_string();

    let compile_result = compiler.compile(&dream_file_str, &wat_path_str);

    if expected_error_file.exists() {
        assert!(
            compile_result.is_err(),
            "Expected compilation to fail for {:?}",
            dream_file
        );
        return;
    }

    compile_result.unwrap_or_else(|e| panic!("Compilation failed for {:?}: {e}", dream_file));

    let expects_trap = expected_trap_file.exists();
    let expected_output = fs::read_to_string(if expects_trap {
        &expected_trap_file
    } else {
        &expected_file
    })
    .unwrap_or_else(|_| panic!("Missing .expected/.expected_trap file for {:?}", dream_file));

    let bin = wat_path.with_extension("out");
    let output = match dream::execution::native_runner::execute_native_capturing(&bin) {
        Ok(stdout) => {
            (
                true,
                stdout,
                String::new(),
            )
        }
        Err(e) => {
            let msg = e.to_string();
            (false, String::new(), msg)
        }
    };
    let (ok, actual_output, stderr) = output;

    if expects_trap {
        assert!(
            !ok,
            "Expected a runtime panic for {:?}, but execution completed normally",
            dream_file
        );
        let combined = format!("{}{}", actual_output, stderr);
        assert!(
            combined.trim().contains(expected_output.trim())
                || combined.trim().starts_with(expected_output.trim())
                || stderr.contains("panic:"),
            "Output mismatch for {:?}\n  expected prefix: {:?}\n  actual: {:?}",
            dream_file,
            expected_output.trim(),
            combined.trim()
        );
    } else {
        assert!(
            ok,
            "Execution failed for {:?}: {}",
            dream_file,
            stderr
        );
        assert_eq!(
            actual_output.trim(),
            expected_output.trim(),
            "Output mismatch for {:?}",
            dream_file
        );
    }

    let _ = fs::remove_file(&wat_path);
    let _ = fs::remove_file(&bin);
    let _ = fs::remove_file(wat_path.with_extension("ll"));
}
/// Collect every `tests/cases/*.dream` fixture path.
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

/// Run fixtures in parallel across all CPU cores. `release_for` decides, per fixture stem,
/// whether the case runs through the release or debug backend. `wat_ext` scopes this suite's
/// generated output files so concurrent suites never collide. Each fixture's failure is captured
/// (rather than aborting the run at the first panic) so one invocation reports every broken case.
/// When `only` is `Some`, only those stems run.
fn run_corpus(wat_ext: &str, release_for: impl Fn(&str) -> bool + Sync, only: Option<&[&str]>) {
    let mut paths = collect_case_paths();
    if let Some(stems) = only {
        paths.retain(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| stems.contains(&s))
        });
    }
    if paths.is_empty() {
        println!("No .dream files found in tests/cases/");
        return;
    }

    let failures: Vec<String> = paths
        .par_iter()
        .filter_map(|path| {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let release = release_for(stem);
            // `run_test_case` signals failure via panic/assert; catch it so rayon aggregates rather
            // than tearing down the whole run on the first failure.
            match catch_unwind(AssertUnwindSafe(|| run_test_case(path, release, wat_ext))) {
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
        "{} e2e case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn run_all_e2e_cases() {
    // Default debug backend (`with_release(false)`).
    run_corpus("wat", |_| false, None);
}

/// The whole suite run through the *release* backend (`with_release(true)`), the only path that
/// enables structural WAT dead-function elimination and the uninstrumented allocator. This guards
/// against a case that passes in debug but breaks in release because DCE trimmed a live function or
/// the hot path diverged. EVERY case runs here with full output asserted: instrumentation-probe
/// cases (`DEBUG_ONLY_CASES`) run in debug (their counts are debug-specific), all others in release.
#[test]
#[ignore = "full release corpus; cargo test --workspace -- --ignored"]
fn run_all_e2e_cases_release() {
    // The instrumentation-probe cases only produce correct output with the debug allocator, so
    // bypass release for them and run them in debug with the full output assertion — they are
    // important and must stay fully checked, not relaxed to a smoke test.
    run_corpus(
        "release.wat",
        |stem| !DEBUG_ONLY_CASES.contains(&stem),
        None,
    );
}

/// Codegen must be reproducible: compiling the same program twice (each compile uses fresh,
/// independently-seeded `HashMap`s within this process) must yield byte-identical `.wat` and
/// (with `--runtime`) `*.web.runtime.js`. This guards the `IndexMap` conversion of the emission-driving
/// tables against regressions that would reintroduce `HashMap`-iteration nondeterminism. Uses
/// release mode so the check covers the production emit path (`strip_dead_functions`).
#[test]
fn codegen_is_deterministic() {
    let cases_dir = Path::new("tests/cases");
    if !cases_dir.exists() {
        return;
    }
    // Two independent compiles of a couple of fixtures is enough to catch HashMap-order
    // regressions; the full debug corpus covers more shapes in the default gate.
    let fixtures = ["classes", "async_basic"];
    for name in fixtures {
        let src = cases_dir.join(format!("{}.dream", name));
        if !src.exists() {
            continue;
        }
        let src_str = src.to_str().unwrap().to_string();
        let mut prev_wat: Option<String> = None;
        let mut prev_rt: Option<String> = None;
        for run in 0..2 {
            let out = std::env::temp_dir().join(format!("dream_det_{}_{}.wat", name, run));
            let out_str = out.to_str().unwrap().to_string();
            Compiler::new(Target::Native)
                .with_release(true)
                .with_optimize(None)
                .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
                .compile(&src_str, &out_str)
                .unwrap_or_else(|_| panic!("Compilation failed for {}", name));
            let wat = fs::read_to_string(out.with_extension("ll")).unwrap();
            let rt_path = out.with_extension("web.runtime.js");
            let rt = fs::read_to_string(&rt_path)
                .unwrap_or_else(|e| panic!("missing selective runtime for {}: {}", name, e));
            let _ = fs::remove_file(&out);
            let _ = fs::remove_file(&rt_path);
            let _ = fs::remove_file(out.with_extension("wasm"));
            let _ = fs::remove_file(out.with_extension("abi.json"));
            if let Some(ref first) = prev_wat {
                assert_eq!(
                    first, &wat,
                    "Nondeterministic codegen for {} (run {})",
                    name, run
                );
            } else {
                prev_wat = Some(wat);
            }
            if let Some(ref first) = prev_rt {
                assert_eq!(
                    first, &rt,
                    "Nondeterministic selective runtime for {} (run {})",
                    name, run
                );
            } else {
                prev_rt = Some(rt);
            }
        }
    }
}

/// `runtime/dream.js` must match a fresh bundle of `runtime/src/` (edit sources, then run
/// `node scripts/bundle-runtime.mjs`).
#[test]
fn dream_js_bundle_is_fresh() {
    let status = std::process::Command::new("node")
        .args(["scripts/bundle-runtime.mjs", "--check"])
        .status()
        .expect("failed to spawn node for bundle-runtime check");
    assert!(
        status.success(),
        "runtime/dream.js is stale; run: node scripts/bundle-runtime.mjs"
    );
}

/// A compute-free arithmetic program must not pull GPU/FS/crypto host chunks into its selective
/// runtime (js bridges may still appear when layouts exist for marshaler keepalive).
#[test]
fn selective_runtime_omits_unused_host_chunks() {
    if !clang_can_emit_wasm32() {
        return;
    }
    let src = Path::new("tests/cases/arithmetic.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_sel_runtime_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::Wasm)
        .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
        .compile(&src_str, &out_str)
        .expect("arithmetic compile");
    let rt = fs::read_to_string(out.with_extension("web.runtime.js")).expect("web.runtime.js");
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(out.with_extension("web.runtime.js"));
    let _ = fs::remove_file(out.with_extension("wasm"));
    let _ = fs::remove_file(out.with_extension("abi.json"));
    assert!(!rt.contains("makeGpuHost"), "gpu chunk should be absent");
    assert!(!rt.contains("makeFsHost"), "fs chunk should be absent");
    assert!(
        !rt.contains("makeCryptoHost"),
        "crypto chunk should be absent"
    );
    assert!(rt.contains("function load("));
}

/// `--release` arithmetic must keep a tiny code section (last-use destroy is compiler-only).
#[test]
fn release_arithmetic_code_section_stays_small() {
    if !clang_can_emit_wasm32() {
        return;
    }
    let src = Path::new("tests/cases/arithmetic.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_arith_size_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::Wasm)
        .with_release(true)
        .compile(&src_str, &out_str)
        .expect("arithmetic --release compile");
    let wasm_path = out.with_extension("wasm");
    let wasm = fs::read(&wasm_path).expect("arithmetic.wasm");
    let code = wasm_code_section_len(&wasm);
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(out.with_extension("abi.json"));
    assert!(
        code > 0 && code <= 2048,
        "arithmetic --release code section should stay under 2KiB (got {})",
        code
    );
}

/// Sample `--release` code-section slack: last-use destroy is compiler-only (no extra runtime
/// helpers). Music player is DOM/`js`-heavy; a large jump here means always-live WAT crept back.
#[test]
fn release_music_player_code_section_stays_bounded() {
    if !clang_can_emit_wasm32() {
        return;
    }
    let src = Path::new("sample/music_player/music_player.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_music_player_size_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::Wasm)
        .with_release(true)
        .compile(&src_str, &out_str)
        .expect("music_player --release compile");
    let wasm_path = out.with_extension("wasm");
    let wasm = fs::read(&wasm_path).expect("music_player.wasm");
    let code = wasm_code_section_len(&wasm);
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(out.with_extension("abi.json"));
    assert!(
        code > 0 && code <= 24 * 1024,
        "music_player --release code section should stay under 24KiB (got {})",
        code
    );
}

fn wasm_code_section_len(data: &[u8]) -> usize {
    if data.len() < 8 || &data[0..4] != b"\0asm" {
        return 0;
    }
    let mut i = 8usize;
    while i < data.len() {
        let id = data[i];
        i += 1;
        let (size, ni) = uleb32(data, i);
        i = ni;
        if id == 10 {
            return size;
        }
        i += size;
        if i > data.len() {
            break;
        }
    }
    0
}

fn uleb32(data: &[u8], mut i: usize) -> (usize, usize) {
    let mut result = 0usize;
    let mut shift = 0;
    while i < data.len() {
        let b = data[i];
        i += 1;
        result |= ((b & 0x7f) as usize) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (result, i)
}

/// `@test` discovery + synthesized runner (`dream test` path).
#[test]
fn dream_test_runs_attr_marked_functions() {
    let path = Path::new("tests/cases/support/attr_tests");
    if !path.exists() {
        return;
    }
    let result = dream::driver::test::run_tests(
        path,
        &dream::driver::test::TestOptions {
            release: false,
            filter: None,
            verbose: false,
        },
    )
    .expect("dream test should succeed");
    assert_eq!(result.files_run, 1);
    assert_eq!(result.tests_run, 3);
}
