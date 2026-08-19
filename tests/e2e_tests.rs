//! Golden e2e: native C (`Target::NativeC` + cc). WAT determinism / JS runtime tests stay below.

use dream::driver::compiler::{Compiler, Target};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::{compile_and_capture, compile_and_capture_ex};
use pretty_assertions::assert_eq;
use rayon::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::thread;

const SMOKE_CASES: &[&str] = &[
    "arithmetic",
    "classes",
    "enum_basic",
    "generic_structs",
    "async_basic",
    "async_generic_sink_reuse",
    "collection_literals",
    "map_basics",
    "interfaces",
    "object_protocol",
    "literal_methods",
    "path_helpers",
    "stdlib_helpers",
    "diagnostics",
    "last_use_destroy",
    "struct_last_use_move",
    "ui_render_tree",
    "simd_f32x4",
    "autovec_arr_add",
];

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

fn spawn_tcp_echo() -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback tcp");
    let port = listener.local_addr().expect("tcp local addr").port();
    let handle = thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 64];
        if let Ok(n) = stream.read(&mut buf) {
            let _ = stream.write_all(&buf[..n]);
        }
    });
    (port, handle)
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
    compile_result.unwrap_or_else(|e| panic!("compile failed for {:?}: {}", dream_file, e));

    let expects_trap = expected_trap_file.exists();
    let expected_output = if expects_trap {
        fs::read_to_string(&expected_trap_file).unwrap_or_default()
    } else if expected_file.exists() {
        fs::read_to_string(&expected_file).unwrap_or_default()
    } else {
        String::new()
    };

    let opt = if release { OptLevel::O3 } else { OptLevel::O0 };
    let c_str = c_path.to_str().unwrap();
    let tcp = if stem == "tcp_echo_local" {
        Some(spawn_tcp_echo())
    } else {
        None
    };
    let timeout_secs = if stem == "http_get_local" { 20 } else { 8 };
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
    let mut env: Vec<(&str, String)> = Vec::new();
    if let Some((port, _)) = tcp.as_ref() {
        env.push(("DREAM_E2E_TCP_PORT", port.to_string()));
    }
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let run = if timeout_secs != 8 || !extra_args.is_empty() || stdin.is_some() || !env_refs.is_empty()
    {
        compile_and_capture_ex(c_str, opt, &env_refs, extra_args, stdin, timeout_secs)
    } else {
        compile_and_capture(c_str, opt)
    };
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(c_path.with_extension("o"));
    let _ = fs::remove_file(c_path.with_extension("bin"));

    if expects_trap {
        assert!(run.is_err(), "expected trap for {:?}", dream_file);
        return;
    }
    let actual = run.unwrap_or_else(|e| panic!("run failed for {:?}: {}", dream_file, e));
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
fn run_smoke_e2e_cases() {
    run_corpus(false, Some(SMOKE_CASES));
}

#[test]
fn run_file_http_parity_e2e() {
    run_corpus(
        false,
        Some(&[
            "file_bytes",
            "file_dir",
            "file_stats",
            "file_copy_rename",
            "file_remove_dir",
            "http_get_local",
            "tcp_echo_local",
            "process_args_basic",
            "console_read_line",
            "crypto_basic",
            "process_run_basic",
            "process_spawn_basic",
            "timezone_basic",
            "tcp_client_connect_fail",
            "websocket_unsupported_scheme",
            "websocket_connect_fail",
        ]),
    );
}

#[test]
#[ignore = "full golden corpus; cargo test --workspace -- --ignored"]
fn run_all_e2e_cases() {
    run_corpus(false, None);
}

#[test]
#[ignore = "full native C release corpus; cargo test --workspace -- --ignored"]
fn run_all_e2e_cases_release() {
    run_corpus(true, None);
}

/// Codegen must be reproducible: compiling the same program twice (each compile uses fresh,
/// independently-seeded `HashMap`s within this process) must yield byte-identical `.wasm` (primary)
/// and printer `.wat` (secondary), plus (with `--runtime`) `*.web.runtime.js`. This guards the
/// `IndexMap` conversion of the emission-driving tables against regressions that would reintroduce
/// `HashMap`-iteration nondeterminism. Uses release mode so the check covers builder DCE.
#[test]
fn codegen_is_deterministic() {
    let cases_dir = Path::new("tests/cases");
    if !cases_dir.exists() {
        return;
    }
    // Two independent compiles of a couple of fixtures is enough to catch HashMap-order
    // regressions; the ignored full corpus still covers more shapes in CI `--ignored` runs.
    let fixtures = ["classes", "async_basic"];
    for name in fixtures {
        let src = cases_dir.join(format!("{}.dream", name));
        if !src.exists() {
            continue;
        }
        let src_str = src.to_str().unwrap().to_string();
        let mut prev_wasm: Option<Vec<u8>> = None;
        let mut prev_wat: Option<String> = None;
        let mut prev_rt: Option<String> = None;
        for run in 0..2 {
            let out = std::env::temp_dir().join(format!("dream_det_{}_{}.wat", name, run));
            let out_str = out.to_str().unwrap().to_string();
            Compiler::new(Target::Wasm)
                .with_release(true)
                .with_optimize(None)
                .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
                .compile(&src_str, &out_str)
                .unwrap_or_else(|_| panic!("Compilation failed for {}", name));
            let wat = fs::read_to_string(&out).unwrap();
            let wasm = fs::read(out.with_extension("wasm")).unwrap();
            let rt_path = out.with_extension("web.runtime.js");
            let rt = fs::read_to_string(&rt_path)
                .unwrap_or_else(|e| panic!("missing selective runtime for {}: {}", name, e));
            let _ = fs::remove_file(&out);
            let _ = fs::remove_file(&rt_path);
            let _ = fs::remove_file(out.with_extension("wasm"));
            let _ = fs::remove_file(out.with_extension("abi.json"));
            if let Some(ref first) = prev_wasm {
                assert_eq!(
                    first, &wasm,
                    "Nondeterministic .wasm for {} (run {})",
                    name, run
                );
            } else {
                prev_wasm = Some(wasm);
            }
            if let Some(ref first) = prev_wat {
                assert_eq!(
                    first, &wat,
                    "Nondeterministic printer WAT for {} (run {})",
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
            ..Default::default()
        },
    )
    .expect("dream test should succeed");
    assert_eq!(result.files_run, 1);
    assert_eq!(result.tests_run, 3);
}
