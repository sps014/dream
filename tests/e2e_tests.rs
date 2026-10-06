//! Golden e2e: native builds (`Target::native()`). WAT determinism / JS runtime tests stay below.

use dream::driver::compiler::Compiler;
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native::compile_and_capture_ex;
use dream_mir::backend::Target;

use crate::common;
use pretty_assertions::assert_eq;
use rayon::prelude::*;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

const SMOKE_CASES: &[&str] = &[
    "arithmetic",
    "pointer_integers",
    "classes",
    "enum_basic",
    "generic_structs",
    "async_basic",
    "async_generic_sink_reuse",
    "collection_literals",
    "array_repeat",
    "map_basics",
    "map_indexer_missing",
    "container_clear_rc",
    "unique_region_tree",
    "cycle_tuple_field",
    "cycle_via_value_struct",
    "cycle_interface_field",
    "receiver_mode_inference",
    "weak_handle_lifecycle",
    "borrow_after_last_use",
    "buffer_clear_truncate",
    "container_rewind_legal",
    "arc_slot_read_retains",
    "string_split_once",
    "interfaces",
    "object_protocol",
    "literal_methods",
    "path_helpers",
    "stdlib_helpers",
    "diagnostics",
    "last_use_destroy",
    "defer_last_use",
    "defer_zero",
    "arc_global_reassign",
    "defer_global_reassign",
    "struct_last_use_move",
    "ui_render_tree",
    "simd_f32x4",
    "autovec_arr_add",
    "heap_large_array",
    "case_negative",
    "case_duplicate",
    "case_runtime_field",
    "switch_bool",
    "nested_self_realloc",
    "literal_overflow",
    "sizeof_unknown",
    "sizeof",
    "sizeof_case_duplicates",
    "guarded_arm_nonreturn",
    "char_literal_errors",
    "defer_break",
    "lock_await_rejected",
    "await_non_async_lambda",
    "await_sync_map_literal",
    "await_sync_tuple_destructure",
    "class_export",
    "class_export_generic",
    "class_export_option",
    "class_export_enum",
    "panic_div_zero",
    "task_basic",
    "task_local_alloc",
    "task_spawn_no_leak",
    "promise_start_no_leak",
    "hello_println",
    "stdio_streams",
    "stdlib_internal_hidden",
    "cancellation_basic",
    "cancellation_stdlib",
    "primary_constructor",
    "unit_variant_bare",
    "union_named_single_field",
    "class_indexer",
    "operator_overloading",
];

fn assert_needles(haystack: &str, expected: &str, dream_file: &Path, kind: &str) {
    let needles: Vec<&str> = expected
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    for needle in needles {
        assert!(
            haystack.contains(needle),
            "{kind} for {:?} missing {:?}\n--- output ---\n{haystack}",
            dream_file,
            needle
        );
    }
}

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

fn run_native_case(dream_file: &Path) {
    let native_expected = dream_file.with_extension("expected.native");
    let expected_file = if Target::native().spec().ptr_size == 8 && native_expected.exists() {
        native_expected
    } else {
        dream_file.with_extension("expected")
    };
    let expected_error_file = dream_file.with_extension("expected_error");
    let expected_trap_file = dream_file.with_extension("expected_trap");
    let stem = dream_file.file_stem().and_then(|s| s.to_str()).unwrap();
    // Smoke and parity suites overlap and run concurrently, including binary cleanup.
    let artifacts = tempfile::Builder::new().prefix(stem).tempdir().unwrap();
    let ll_path = artifacts.path().join(format!("{stem}.ll"));
    let compiler = Compiler::new(Target::native());
    let src = dream_file.to_str().unwrap().to_string();
    let dest = ll_path.to_str().unwrap().to_string();
    let compile_result = compiler.compile(&src, &dest);

    if expected_error_file.exists() {
        let err = match compile_result {
            Err(e) => e,
            Ok(_) => panic!("Expected compilation to fail for {:?}", dream_file),
        };
        let rendered = err.diagnostic_text().unwrap_or("").to_string();
        let expected = fs::read_to_string(&expected_error_file).unwrap_or_default();
        assert_needles(&rendered, &expected, dream_file, "compile error");
        let _ = fs::remove_file(&ll_path);
        let _ = fs::remove_file(ll_path.with_extension("o"));
        return;
    }
    compile_result.unwrap_or_else(|e| panic!("compile failed for {:?}: {}", dream_file, e));

    let expects_trap = expected_trap_file.exists();
    let expected_output = if expects_trap {
        fs::read_to_string(&expected_trap_file).unwrap_or_default()
    } else if expected_file.exists() {
        fs::read_to_string(&expected_file).unwrap_or_default()
    } else {
        String::new()
    };

    let ll_str = ll_path.to_str().unwrap();
    let timeout_secs = 30;
    let extra_args: &[&str] = if stem == "process_args_basic" {
        &["alpha", "beta"]
    } else {
        &[]
    };
    let stdin = if stem == "console_read_line" {
        Some(&b"hello-line\n"[..])
    } else {
        None
    };
    let run = compile_and_capture_ex(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        ll_str,
        OptLevel::O0,
        &[],
        extra_args,
        stdin,
        timeout_secs,
    );
    let _ = fs::remove_file(&ll_path);
    let _ = fs::remove_file(ll_path.with_extension("o"));
    let _ = fs::remove_file(ll_path.with_extension("bin"));

    if expects_trap {
        let err = match run {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected trap for {:?}", dream_file),
        };
        assert_needles(&err, &expected_output, dream_file, "trap");
        return;
    }
    let actual = run.unwrap_or_else(|e| panic!("run failed for {:?}: {}", dream_file, e));
    let actual = common::normalize_stdout(actual);
    assert_eq!(
        actual.trim(),
        expected_output.trim(),
        "Output mismatch for {:?}",
        dream_file
    );
}

fn run_corpus(only: Option<&[&str]>) {
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
        .filter_map(
            |path| match catch_unwind(AssertUnwindSafe(|| run_native_case(path))) {
                Ok(()) => None,
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown panic".to_string());
                    Some(format!("{:?}: {}", path, msg))
                }
            },
        )
        .collect();
    assert!(
        failures.is_empty(),
        "{} native e2e case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn wasm_runner_script(runtime: &Path, wasm: &Path) -> String {
    format!(
        "import {{ pathToFileURL }} from 'node:url';\n\
         const {{ run }} = await import(pathToFileURL({js}).href);\n\
         const timer = setTimeout(() => {{ console.error('wasm/js e2e timeout'); process.exit(2); }}, 25000);\n\
         run({wasm}, {{ stdout: (s) => process.stdout.write(s) }}).await;\n\
         clearTimeout(timer);\n",
        js = js_string(runtime.to_str().unwrap()),
        wasm = js_string(wasm.to_str().unwrap()),
    )
}

#[test]
fn file_urls_round_trip_special_paths() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("runtime #π%.mjs");
    fs::write(
        &file,
        "export function run(path, {stdout}) { stdout(path); return {await: 0}; }\n",
    )
    .unwrap();
    let wasm = dir.path().join("guest #π%.wasm");
    let runner = dir.path().join("runner.mjs");
    fs::write(&runner, wasm_runner_script(&file, &wasm)).unwrap();
    let output = Command::new("node").arg(runner).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        wasm.to_str().unwrap()
    );
}

fn run_wasm_js_case(dream_file: &Path) {
    let expected_file = dream_file.with_extension("expected");
    if !expected_file.exists() {
        return;
    }
    let stem = dream_file.file_stem().and_then(|s| s.to_str()).unwrap();
    let dest_dir = Path::new("target").join("e2e-wasm32").join(stem);
    fs::create_dir_all(&dest_dir).unwrap();
    let wat_path = dest_dir.join(format!("{stem}.wat"));
    let src = dream_file.to_str().unwrap().to_string();
    let dest = wat_path.to_str().unwrap().to_string();
    Compiler::new(Target::wasm32())
        .compile(&src, &dest)
        .unwrap_or_else(|e| panic!("wasm compile failed for {:?}: {}", dream_file, e));
    let wasm_path = wat_path.with_extension("wasm");
    let wasm_path = fs::canonicalize(&wasm_path).unwrap_or(wasm_path);
    let dream_js = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/dream.js");
    let runner = dest_dir.join(format!("{stem}_run.mjs"));
    fs::write(&runner, wasm_runner_script(&dream_js, &wasm_path)).unwrap();
    let child = Command::new("node")
        .arg(&runner)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("node failed for {:?}: {}", dream_file, e));
    let pid = child.id();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    thread::spawn(move || {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(30) {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        if !flag.load(std::sync::atomic::Ordering::Relaxed) {
            if cfg!(windows) {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .status();
            } else {
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        }
    });
    let out = child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("node failed for {:?}: {}", dream_file, e));
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        out.status.success(),
        "node run failed for {:?}: {}\n{}",
        dream_file,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let expected = fs::read_to_string(&expected_file).unwrap();
    let actual = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "wasm/js output mismatch for {:?}",
        dream_file
    );
}

fn wasm_js_success_stems() -> Vec<String> {
    let mut stems = vec![
        "println_basic".into(),
        "arithmetic".into(),
        "async_basic".into(),
    ];
    if let Ok(rd) = fs::read_dir("tests/cases") {
        let mut extra: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let stem = p.file_stem()?.to_str()?.to_string();
                if p.extension()?.to_str()? != "dream" {
                    return None;
                }
                if !p.with_extension("expected").exists() {
                    return None;
                }
                let jsonish =
                    stem.contains("json") || stem == "struct_json" || stem == "tuple_json";
                if stem.starts_with("task_") || jsonish {
                    Some(stem)
                } else {
                    None
                }
            })
            .collect();
        extra.sort();
        extra.dedup();
        stems.extend(extra);
    }
    stems
}

#[test]
fn run_wasm_js_smoke_e2e() {
    let stems = wasm_js_success_stems();
    let failures: Vec<String> = stems
        .par_iter()
        .filter_map(|stem| {
            let path = Path::new("tests/cases").join(format!("{stem}.dream"));
            match catch_unwind(AssertUnwindSafe(|| run_wasm_js_case(&path))) {
                Ok(()) => None,
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown panic".to_string());
                    Some(format!("{stem}: {msg}"))
                }
            }
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} wasm/js e2e case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn wasm_js_compile_errors_match_native() {
    for stem in [
        "task_value_struct_capture_violation",
        "js_capturing_lambda_func",
        "js_capturing_lambda_slot",
    ] {
        let src = Path::new("tests/cases").join(format!("{stem}.dream"));
        if !src.with_extension("expected_error").exists() {
            continue;
        }
        let dest = std::env::temp_dir().join(format!("dream_wasm_err_{stem}.wat"));
        let src_s = src.to_str().unwrap().to_string();
        let dest_s = dest.to_str().unwrap().to_string();
        let err = Compiler::new(Target::wasm32()).compile(&src_s, &dest_s);
        assert!(err.is_err(), "{} should fail to compile for wasm", stem);
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(dest.with_extension("ll"));
        let _ = fs::remove_file(dest.with_extension("wasm"));
    }
}

#[test]
fn wasm_compiles_js_interop_samples() {
    for rel in [
        "sample/interop/js.dream",
        "sample/interop/callbacks.dream",
        "sample/interop/async_js.dream",
        "sample/interop/slots.dream",
        "sample/interop/structs.dream",
        "sample/interop/value_structs.dream",
        "sample/interop/option_fields.dream",
    ] {
        let src = Path::new(rel);
        if !src.exists() {
            continue;
        }
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap();
        let dest = std::env::temp_dir().join(format!("dream_js_{stem}.wat"));
        let src_s = src.to_str().unwrap().to_string();
        let dest_s = dest.to_str().unwrap().to_string();
        Compiler::new(Target::wasm32())
            .compile(&src_s, &dest_s)
            .unwrap_or_else(|e| panic!("{} should compile to wasm32: {}", rel, e));
        let wasm = dest.with_extension("wasm");
        assert!(wasm.is_file(), "expected {}", wasm.display());
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(&wasm);
        let _ = fs::remove_file(dest.with_extension("ll"));
        let _ = fs::remove_file(dest.with_extension("abi.json"));
    }
}

#[test]
fn wasm32_js_option_struct_fields_are_marshaled() {
    let src = Path::new("sample/interop/option_fields.dream");
    let dest = std::env::temp_dir().join("dream_js_option_fields.wat");
    let src_s = src.to_str().unwrap().to_string();
    let dest_s = dest.to_str().unwrap().to_string();
    Compiler::new(Target::wasm32())
        .compile(&src_s, &dest_s)
        .unwrap_or_else(|e| panic!("option_fields should compile to wasm32: {}", e));
    let ll_path = dest.with_extension("ll");
    let ll =
        fs::read_to_string(&ll_path).unwrap_or_else(|e| panic!("read {}: {e}", ll_path.display()));
    assert!(
        ll.contains("jsIsNull") && ll.contains("jsNull"),
        "Option fields must marshal None as JS null, got marshaler without jsIsNull/jsNull:\n{}",
        ll
    );
    assert!(
        ll.contains("Profile_to_js"),
        "expected Profile_to_js marshaler:\n{}",
        ll
    );
    let _ = fs::remove_file(&dest);
    let _ = fs::remove_file(dest.with_extension("wasm"));
    let _ = fs::remove_file(&ll_path);
    let _ = fs::remove_file(dest.with_extension("abi.json"));
}

#[test]
fn run_smoke_e2e_cases() {
    run_corpus(Some(SMOKE_CASES));
}

#[test]
fn run_core_services_e2e() {
    run_corpus(Some(&[
        "file_bytes",
        "file_dir",
        "file_stats",
        "file_copy_rename",
        "file_remove_dir",
        "process_args_basic",
        "console_read_line",
        "crypto_basic",
        "process_run_basic",
        "process_spawn_basic",
        "timezone_basic",
    ]));
}

/// Native ASan/LSan on leak-sensitive goldens (see `src/execution/native`). Guest `live=0` is
/// still the heap-counter check. The sanitizer is selected by the environment the compiles
/// inherit, which this process shares with every other test, so the caller sets it.
#[test]
#[ignore = "native sanitizer; DREAM_NATIVE_SANITIZE=address,leak ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 cargo test --test integration e2e_tests::native_asan_focused_goldens -- --ignored --exact"]
fn native_asan_focused_goldens() {
    if std::env::var_os("DREAM_NATIVE_SANITIZE").is_none() {
        eprintln!("skipping: set DREAM_NATIVE_SANITIZE=address,leak");
        return;
    }
    run_corpus(Some(&[
        "json_parse",
        "json_roundtrip",
        "promise_start_no_leak",
    ]));
}

/// Codegen must be reproducible: compiling the same program twice (each compile uses fresh,
/// independently-seeded `HashMap`s within this process) must yield byte-identical `.ll`, `.wasm`
/// and (with `--runtime`) `*.web.runtime.js`. This guards the `IndexMap` conversion of the
/// emission-driving tables against regressions that would reintroduce `HashMap`-iteration
/// nondeterminism. Both runs write the same path: the output name is part of the module.
#[test]
fn codegen_is_deterministic() {
    let cases_dir = Path::new("tests/cases");
    if !cases_dir.exists() {
        return;
    }
    for name in ["classes", "async_basic"] {
        let src = cases_dir.join(format!("{}.dream", name));
        if src.exists() {
            assert_deterministic("dream_det", name, &src, true);
        }
    }
}

/// [`codegen_is_deterministic`] over every golden that compiles for wasm32. Cases that fail to
/// compile (native-only hosts, `.expected_error`) are skipped: this checks reproducibility only.
#[test]
#[ignore = "full corpus, two compiles per case; cargo test --test integration e2e_tests::codegen_is_deterministic_full_corpus -- --ignored"]
fn codegen_is_deterministic_full_corpus() {
    let mut names: Vec<String> = fs::read_dir("tests/cases")
        .expect("tests/cases")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "dream" && p.with_extension("expected").exists())
                .then(|| p.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(name) = names.get(i) else {
                    break;
                };
                let src = Path::new("tests/cases").join(format!("{}.dream", name));
                assert_deterministic("dream_det_all", name, &src, false);
            });
        }
    });
}

/// Compiles `src` twice to the same path and asserts byte-identical `.ll`, `.wasm` and
/// `*.web.runtime.js`. With `must_compile` false, a failing first compile skips the case.
/// `prefix` keeps concurrently running tests off each other's output files.
fn assert_deterministic(prefix: &str, name: &str, src: &Path, must_compile: bool) {
    let src_str = src.to_str().unwrap().to_string();
    let out = std::env::temp_dir().join(format!("{}_{}.wat", prefix, name));
    let out_str = out.to_str().unwrap().to_string();
    let artifacts = [
        out.with_extension("ll"),
        out.with_extension("wasm"),
        out.with_extension("web.runtime.js"),
    ];
    let mut prev: Option<Vec<Vec<u8>>> = None;
    for run in 0..2 {
        let result = Compiler::new(Target::wasm32())
            .with_release(true)
            .with_optimize(None)
            .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
            .compile(&src_str, &out_str);
        if result.is_err() {
            assert!(!must_compile, "Compilation failed for {}", name);
            assert!(prev.is_none(), "{} compiled once, then failed", name);
            break;
        }
        let bytes: Vec<Vec<u8>> = artifacts
            .iter()
            .map(|p| fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
            .collect();
        if let Some(first) = &prev {
            for (i, p) in artifacts.iter().enumerate() {
                assert!(
                    first[i] == bytes[i],
                    "Nondeterministic {} for {} (run {})",
                    p.display(),
                    name,
                    run
                );
            }
        } else {
            prev = Some(bytes);
        }
    }
    for p in artifacts
        .iter()
        .chain([&out, &out.with_extension("abi.json")])
    {
        let _ = fs::remove_file(p);
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

#[test]
fn dream_js_bundle_is_platform_independent() {
    let status = Command::new("node")
        .args(["--test", "scripts/test-bundle-runtime.mjs"])
        .status()
        .unwrap();
    assert!(
        status.success(),
        "runtime bundle differs across platform paths"
    );
}

/// An arithmetic program must not pull FS/crypto host chunks into its selective
/// runtime (js bridges may still appear when layouts exist for marshaler keepalive).
#[test]
fn selective_runtime_omits_unused_host_chunks() {
    let src = Path::new("tests/cases/arithmetic.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_sel_runtime_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::wasm32())
        .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
        .compile(&src_str, &out_str)
        .expect("arithmetic compile");
    let rt = fs::read_to_string(out.with_extension("web.runtime.js")).expect("web.runtime.js");
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(out.with_extension("web.runtime.js"));
    let _ = fs::remove_file(out.with_extension("wasm"));
    let _ = fs::remove_file(out.with_extension("abi.json"));
    assert!(!rt.contains("makeFsHost"), "fs chunk should be absent");
    assert!(
        !rt.contains("makeCryptoHost"),
        "crypto chunk should be absent"
    );
    assert!(rt.contains("function load("));
}

#[test]
fn release_arithmetic_omits_worker_publication() {
    let src = Path::new("tests/cases/arithmetic.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_arith_release_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::wasm32())
        .with_release(true)
        .compile(&src_str, &out_str)
        .expect("arithmetic --release compile");
    let wasm_path = out.with_extension("wasm");
    let wat = fs::read_to_string(&out).expect("arithmetic WAT");
    wat::parse_str(&wat).expect("arithmetic WAT must parse");
    assert!(
        !wat.contains("(export \"dream_publish\""),
        "worker-only publication must not be exported by arithmetic"
    );
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(out.with_extension("abi.json"));
}

#[test]
fn release_music_player_compiles_to_wasm() {
    let src = Path::new("sample/music_player/music_player.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_music_player_release_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::wasm32())
        .with_release(true)
        .compile(&src_str, &out_str)
        .expect("music_player --release compile");
    let wasm_path = out.with_extension("wasm");
    wat::parse_file(&out).expect("music_player WAT must parse");
    assert!(
        wasm_path.is_file(),
        "music_player must produce a WASM module"
    );
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(out.with_extension("abi.json"));
}

/// `@test` discovery + synthesized runner (`dream test` path).
#[test]
fn dream_test_runs_attr_marked_functions() {
    let path = Path::new("tests/cases/support/attr_tests");
    if !path.exists() {
        return;
    }
    let result = dream::driver::test::run_tests(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        path,
        &dream::driver::test::TestOptions {
            release: false,
            filter: None,
            verbose: false,
            ..Default::default()
        },
    )
    .expect("dream test should succeed");
    assert_eq!(result.files_run, 1);
    assert_eq!(result.tests_run, 3);
}
