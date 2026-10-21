//! Golden e2e: native builds (`Target::native()`). WAT determinism / JS runtime tests stay below.

use dream::driver::compiler::Compiler;
use dream_mir::backend::Target;

use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use std::process::Command;

const SMOKE_CASES: &[&str] = &[
    "operators",
    "class_basics",
    "interfaces",
    "async_basic",
    "task_basic",
    "collection_literals",
    "list_collection",
    "string_methods",
    "path_helpers",
    "stdlib_helpers",
    "arc_param",
    "arc_complete",
    "arc_global_reassign",
    "last_use_destroy",
    "defer_last_use",
    "ui_render_tree",
    "promise_start_no_leak",
    "stdio_pipe",
    "isolated_ownership",
    "operator_overloading",
    "cycle_errors",
    "parse_errors",
    "await_defer_errors",
    "class_export",
    "panic_div_zero",
];

fn run_corpus(only: Option<&[&str]>) {
    crate::corpus::run::run_selected(only, &[dream::driver::test::TestHost::Native]);
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

pub(crate) fn wasm_runner_script(runtime: &Path, wasm: &Path) -> String {
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

#[test]
fn run_wasm_js_smoke_e2e() {
    crate::corpus::run::run_selected(
        Some(&["operators", "async_basic", "task_basic", "json_roundtrip"]),
        &[dream::driver::test::TestHost::Node],
    );
}

#[test]
fn wasm_js_compile_errors_match_native() {
    crate::corpus::run::run_selected(
        Some(&["task_capture_errors", "js_lambda_errors"]),
        &[dream::driver::test::TestHost::Node],
    );
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
        if stem == "option_fields" {
            let ll = fs::read_to_string(dest.with_extension("ll")).unwrap();
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
        }
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(&wasm);
        let _ = fs::remove_file(dest.with_extension("ll"));
        let _ = fs::remove_file(dest.with_extension("abi.json"));
    }
}

#[test]
fn run_smoke_e2e_cases() {
    run_corpus(Some(SMOKE_CASES));
}

#[test]
fn run_core_services_e2e() {
    run_corpus(Some(&[
        "file_dir",
        "isolated_files",
        "process_args_basic",
        "console_read_line",
        "crypto_basic",
        "process_run_basic",
        "datetime",
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
    run_corpus(Some(&["json_roundtrip", "promise_start_no_leak"]));
}

/// Codegen must be reproducible: compiling the same program twice (each compile uses fresh,
/// independently-seeded `HashMap`s within this process) must yield byte-identical `.ll`, `.wasm`
/// and (with `--runtime`) `*.web.runtime.js`. This guards the `IndexMap` conversion of the
/// emission-driving tables against regressions that would reintroduce `HashMap`-iteration
/// nondeterminism. Both runs write the same path: the output name is part of the module.
#[test]
fn codegen_is_deterministic() {
    for name in ["arc_cycles", "arc_cycles_dynamic"] {
        let src = crate::common::compiler_fixture(name);
        assert_deterministic("dream_det", name, &src, None);
    }
}

/// Every Node-capable project and isolated program must produce reproducible artifacts.
#[test]
#[ignore = "full corpus determinism; two builds per project or isolated fixture"]
fn codegen_is_deterministic_full_corpus() {
    use crate::corpus::Expectation;
    let mut projects = std::collections::BTreeMap::<&str, Vec<&crate::corpus::Case>>::new();
    for case in crate::corpus::cases().filter(|case| case.node) {
        match case.expectation {
            Expectation::Error { .. } => {}
            Expectation::Project(suite) => projects.entry(suite).or_default().push(case),
            _ => assert_deterministic("dream_det_all", case.stem, &case.source(), None),
        }
    }
    for (suite, cases) in projects {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/projects")
            .join(suite);
        let mut source = String::from("import system;\nimport system.testing;\n");
        let mut calls = Vec::new();
        for case in cases {
            source.push_str(&format!("import {};\n", case.stem));
            let text = fs::read_to_string(case.source()).unwrap();
            let file = dream::driver::test::discover_tests_in_source(
                &case.source().to_string_lossy(),
                &text,
            )
            .unwrap();
            for test in &file.tests {
                calls.push(format!(
                    "Test.{}(\"{}\", {}){};\n",
                    if test.is_async { "run_async" } else { "run" },
                    test.name,
                    test.name,
                    if test.is_async { ".await" } else { "" }
                ));
            }
        }
        source.push_str("@unsafe\nasync fun main(): void {\n");
        for call in calls {
            source.push_str(&call);
        }
        source.push_str("}\n");
        assert_deterministic(
            "dream_det_all",
            suite,
            &root.join("__dream_test_runner__.dream"),
            Some(&source),
        );
    }
}

/// Compiles `src` twice to the same path and asserts byte-identical `.ll`, `.wasm` and
/// the corresponding selective runtime. Project runners use the Node target.
/// `prefix` keeps concurrently running tests off each other's output files.
fn assert_deterministic(prefix: &str, name: &str, src: &Path, source: Option<&str>) {
    let src_str = src.to_str().unwrap().to_string();
    let out = std::env::temp_dir().join(format!("{}_{}.wat", prefix, name));
    let out_str = out.to_str().unwrap().to_string();
    let runtime = if source.is_some() {
        dream::driver::js_runtime::JsRuntimeTarget::Node
    } else {
        dream::driver::js_runtime::JsRuntimeTarget::Web
    };
    let artifacts = [
        out.clone(),
        out.with_extension("abi.json"),
        out.with_extension("ll"),
        out.with_extension("wasm"),
        out.with_extension(runtime.runtime_extension()),
    ];
    let mut prev: Option<Vec<Vec<u8>>> = None;
    for run in 0..2 {
        let mut compiler = Compiler::new(Target::wasm32())
            .with_release(true)
            .with_runtimes(vec![runtime]);
        if let Some(source) = source {
            compiler = compiler
                .with_virtual_entry(source.to_string())
                .with_compile_targets(dream_abi::attributes::CompileTargets {
                    native: false,
                    node: true,
                    web: false,
                });
        }
        compiler
            .compile(&src_str, &out_str)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
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
    for p in &artifacts {
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

/// An arithmetic program must not pull FS/crypto host chunks into its selective runtime (js
/// bridges may still appear when layouts exist for marshaler keepalive), nor export the
/// worker-only publication entry.
#[test]
fn release_arithmetic_omits_unused_host_chunks_and_publication() {
    let src = &crate::common::compiler_fixture("arithmetic");
    let out = std::env::temp_dir().join("dream_arith_release_check.wat");
    let out_str = out.to_str().unwrap().to_string();
    let src_str = src.to_str().unwrap().to_string();
    Compiler::new(Target::wasm32())
        .with_release(true)
        .with_runtimes(vec![dream::driver::js_runtime::JsRuntimeTarget::Web])
        .compile(&src_str, &out_str)
        .expect("arithmetic --release compile");
    let rt = fs::read_to_string(out.with_extension("web.runtime.js")).expect("web.runtime.js");
    let wat = fs::read_to_string(&out).expect("arithmetic WAT");
    for artifact in [
        out.clone(),
        out.with_extension("web.runtime.js"),
        out.with_extension("wasm"),
        out.with_extension("abi.json"),
    ] {
        let _ = fs::remove_file(artifact);
    }
    assert!(!rt.contains("makeFsHost"), "fs chunk should be absent");
    assert!(
        !rt.contains("makeCryptoHost"),
        "crypto chunk should be absent"
    );
    assert!(rt.contains(" as load,"), "runtime must export load");
    wat::parse_str(&wat).expect("arithmetic WAT must parse");
    assert!(
        !wat.contains("(export \"dream_publish\""),
        "worker-only publication must not be exported by arithmetic"
    );
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
