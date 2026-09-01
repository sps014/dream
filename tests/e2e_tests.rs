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
use std::process::Command;
use std::thread;

const SMOKE_CASES: &[&str] = &[
    "arithmetic",
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
    "webworker_basic",
    "webworker_local_alloc",
    "webworker_spawn_no_leak",
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

struct MockRequest {
    method: String,
    path: String,
    x_tag: Option<String>,
    content_type: Option<String>,
    body: String,
}

/// Reads one HTTP/1.1 request (head + Content-Length framed body); `None` on EOF/error.
fn read_http_request(stream: &mut std::net::TcpStream) -> Option<MockRequest> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(pos) = buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
        {
            break pos;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut content_length = 0usize;
    let mut x_tag = None;
    let mut content_type = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "x-tag" => x_tag = Some(value.to_string()),
            "content-type" => content_type = Some(value.to_string()),
            _ => {}
        }
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Some(MockRequest {
        method,
        path,
        x_tag,
        content_type,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

/// Loopback HTTP mock: echoes `METHOD path|x-tag|content-type|body` as the response body.
/// `/bytes` serves a fixed binary payload instead. Handles sequential requests forever.
fn spawn_http_mock() -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback http");
    let port = listener.local_addr().expect("http local addr").port();
    let handle = thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            while let Some(req) = read_http_request(&mut stream) {
                let body = if req.path == "/bytes" {
                    "bin-data-01".to_string()
                } else {
                    format!(
                        "{} {}|{}|{}|{}",
                        req.method,
                        req.path,
                        req.x_tag.as_deref().unwrap_or("-"),
                        req.content_type.as_deref().unwrap_or("-"),
                        req.body
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    if req.method == "HEAD" { "" } else { body.as_str() }
                );
                if stream.write_all(response.as_bytes()).is_err() {
                    break;
                }
            }
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
        let err = match compile_result {
            Err(e) => e,
            Ok(()) => panic!("Expected compilation to fail for {:?}", dream_file),
        };
        let rendered = err
            .diagnostic_text()
            .unwrap_or("")
            .to_string();
        let expected = fs::read_to_string(&expected_error_file).unwrap_or_default();
        assert_needles(&rendered, &expected, dream_file, "compile error");
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
    let http_mock = if stem == "http_methods_local" {
        Some(spawn_http_mock())
    } else {
        None
    };
    let timeout_secs = if stem == "http_get_local" || stem == "http_methods_local" {
        20
    } else {
        8
    };
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
    if let Some((port, _)) = http_mock.as_ref() {
        env.push(("DREAM_E2E_HTTP_PORT", port.to_string()));
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
        let err = match run {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected trap for {:?}", dream_file),
        };
        assert_needles(&err, &expected_output, dream_file, "trap");
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

fn file_url(path: &Path) -> String {
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", abs.display())
}

fn js_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' | '"' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
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
    Compiler::new(Target::Wasm32)
        .compile(&src, &dest)
        .unwrap_or_else(|e| panic!("wasm compile failed for {:?}: {}", dream_file, e));
    let wasm_path = wat_path.with_extension("wasm");
    let wasm_path = fs::canonicalize(&wasm_path).unwrap_or(wasm_path);
    let dream_js = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/dream.js");
    let runner = dest_dir.join(format!("{stem}_run.mjs"));
    fs::write(
        &runner,
        format!(
            "import {{ run }} from {js};\n\
             const chunks = [];\n\
             const timer = setTimeout(() => {{ console.error('wasm/js e2e timeout'); process.exit(2); }}, 25000);\n\
             await run({wasm}, {{ stdout: (s) => chunks.push(s) }});\n\
             clearTimeout(timer);\n\
             process.stdout.write(chunks.join(\"\"));\n",
            js = js_string(&file_url(&dream_js)),
            wasm = js_string(wasm_path.to_str().unwrap()),
        ),
    )
    .unwrap();
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
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
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
                let jsonish = stem.contains("json") || stem == "struct_json" || stem == "tuple_json";
                if stem.starts_with("webworker_") || jsonish {
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
        "webworker_value_struct_capture_violation",
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
        let err = Compiler::new(Target::Wasm32).compile(&src_s, &dest_s);
        assert!(
            err.is_err(),
            "{} should fail to compile for wasm",
            stem
        );
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(dest.with_extension("c"));
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
        Compiler::new(Target::Wasm32)
            .compile(&src_s, &dest_s)
            .unwrap_or_else(|e| panic!("{} should compile to wasm32 C: {}", rel, e));
        let wasm = dest.with_extension("wasm");
        assert!(wasm.is_file(), "expected {}", wasm.display());
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(&wasm);
        let _ = fs::remove_file(dest.with_extension("c"));
        let _ = fs::remove_file(dest.with_extension("abi.json"));
    }
}

#[test]
fn wasm32_js_option_struct_fields_are_marshaled() {
    let src = Path::new("sample/interop/option_fields.dream");
    let dest = std::env::temp_dir().join("dream_js_option_fields.wat");
    let src_s = src.to_str().unwrap().to_string();
    let dest_s = dest.to_str().unwrap().to_string();
    Compiler::new(Target::Wasm32)
        .compile(&src_s, &dest_s)
        .unwrap_or_else(|e| panic!("option_fields should compile to wasm32: {}", e));
    let c_path = dest.with_extension("c");
    let c = fs::read_to_string(&c_path).unwrap_or_else(|e| panic!("read {}: {e}", c_path.display()));
    assert!(
        c.contains("jsIsNull") && c.contains("jsNull"),
        "Option fields must marshal None as JS null, got marshaler without jsIsNull/jsNull:\n{}",
        c
    );
    assert!(
        c.contains("Profile_to_js"),
        "expected Profile_to_js marshaler:\n{}",
        c
    );
    let _ = fs::remove_file(&dest);
    let _ = fs::remove_file(dest.with_extension("wasm"));
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(dest.with_extension("abi.json"));
}

#[test]
fn wasm_compiles_webgpu_samples() {
    for rel in [
        "sample/compute/gpu_ext.dream",
        "sample/compute/saxpy.dream",
        "sample/compute/gpu_advanced_math.dream",
        "sample/compute/life/life.dream",
    ] {
        let src = Path::new(rel);
        if !src.exists() {
            continue;
        }
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap();
        let dest = std::env::temp_dir().join(format!("dream_gpu_{stem}.wat"));
        let src_s = src.to_str().unwrap().to_string();
        let dest_s = dest.to_str().unwrap().to_string();
        Compiler::new(Target::Wasm32)
            .compile(&src_s, &dest_s)
            .unwrap_or_else(|e| panic!("{} should compile to wasm32 C: {}", rel, e));
        let wasm = dest.with_extension("wasm");
        assert!(wasm.is_file(), "expected {}", wasm.display());
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(&wasm);
        let _ = fs::remove_file(dest.with_extension("c"));
        let _ = fs::remove_file(dest.with_extension("abi.json"));
        let _ = fs::remove_file(dest.with_extension("wgsl"));
    }
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
            "http_methods_local",
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
/// independently-seeded `HashMap`s within this process) must yield byte-identical guest C and
/// (with `--runtime`) `*.web.runtime.js`. Linked `.wasm` is clang/wasm-ld output and is not
/// required to be bit-identical. This guards the `IndexMap` conversion of the emission-driving
/// tables against regressions that would reintroduce `HashMap`-iteration nondeterminism.
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
        let mut prev_c: Option<String> = None;
        let mut prev_rt: Option<String> = None;
        for run in 0..2 {
            let out = std::env::temp_dir().join(format!("dream_det_{}_{}.wat", name, run));
            let out_str = out.to_str().unwrap().to_string();
            Compiler::new(Target::Wasm32)
                .with_release(true)
                .with_optimize(None)
                .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
                .compile(&src_str, &out_str)
                .unwrap_or_else(|_| panic!("Compilation failed for {}", name));
            let c_src = fs::read_to_string(out.with_extension("c")).unwrap();
            let rt_path = out.with_extension("web.runtime.js");
            let rt = fs::read_to_string(&rt_path)
                .unwrap_or_else(|e| panic!("missing selective runtime for {}: {}", name, e));
            let _ = fs::remove_file(&out);
            let _ = fs::remove_file(&rt_path);
            let _ = fs::remove_file(out.with_extension("wasm"));
            let _ = fs::remove_file(out.with_extension("c"));
            let _ = fs::remove_file(out.with_extension("abi.json"));
            if let Some(ref first) = prev_c {
                assert_eq!(
                    first, &c_src,
                    "Nondeterministic C for {} (run {})",
                    name, run
                );
            } else {
                prev_c = Some(c_src);
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
    Compiler::new(Target::Wasm32)
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
    Compiler::new(Target::Wasm32)
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
        code > 0 && code <= 4 * 1024,
        "arithmetic --release code section should stay under 4KiB (got {})",
        code
    );
}

/// Sample `--release` code-section slack: last-use destroy is compiler-only (no extra runtime
/// helpers). Music player is DOM/`js`-heavy; host-routed `js` RC and the exported `dream_ft_get`
/// sit near 38KiB since the unavailable-source handling landed (on_error + transient status line
/// added ~10KiB of guest code), while a gross codegen regression (e.g. printf machinery getting
/// linked for basic to_string calls, or the old un-routed host RC path) must trip.
#[test]
fn release_music_player_code_section_stays_bounded() {
    let src = Path::new("sample/music_player/music_player.dream");
    if !src.exists() {
        return;
    }
    let out = std::env::temp_dir().join("dream_music_player_size_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::Wasm32)
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
        code > 0 && code <= 48 * 1024,
        "music_player --release code section should stay under 48KiB (got {})",
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
