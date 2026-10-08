#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn strong_cycles_require_explicit_teardown() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/sys/native");
    let temp = tempfile::tempdir().expect("ownership test directory");
    let binary = temp.path().join("ownership");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args([
        "-std=gnu11",
        "-O2",
        "-pthread",
        "-DDREAM_RUNTIME_COUNTERS=1",
    ]);
    if let Ok(sanitizer) = std::env::var("DREAM_WORKER_SANITIZER") {
        assert!(matches!(sanitizer.as_str(), "thread" | "address,undefined"));
        command.arg(format!("-fsanitize={sanitizer}")).arg("-g");
    }
    let runtime = native.parent().unwrap().parent().unwrap();
    command
        .arg("-I")
        .arg(runtime.join("core/include"))
        .arg("-I")
        .arg(native.join("include"))
        .arg("-I")
        .arg(runtime.join("include"))
        .arg(runtime.join("core/platform.c"))
        .arg(runtime.join("core/utf8.c"))
        .arg(native.join("platform.c"));
    for unit in [
        "heap.c",
        "ownership.c",
        "heap_maps.c",
        "publish.c",
        "region.c",
        "weak.c",
        "sync.c",
        "strings.c",
    ] {
        let path = dream_mir::runtime::native_runtime_units(
            runtime,
            dream_mir::runtime::RuntimeNeed::CORE,
        )
        .into_iter()
        .find(|entry| entry.path.file_name().is_some_and(|name| name == unit))
        .unwrap()
        .path;
        command.arg(path);
    }
    let build = command
        .arg(root.join("tests/runtime_ownership.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for ownership regressions");
    assert!(
        build.status.success(),
        "ownership harness failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(binary).output().expect("ownership harness");
    assert!(
        output.status.success(),
        "ownership harness failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"plain ARC ownership passed\n");
}

#[test]
fn debug_reports_an_acknowledged_strong_cycle_instead_of_collecting_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().expect("leak diagnostic directory");
    let source = std::fs::read_to_string(root.join("tests/cases/arc_strong_cycle_leak.dream"))
        .unwrap()
        .replace(
            "System.panic(\"intentional strong-cycle leak\");",
            "System.println(\"intentional strong-cycle leak\");",
        );
    let input = temp.path().join("leak.dream");
    let ir = temp.path().join("leak.ll");
    std::fs::write(&input, source).unwrap();
    dream::driver::compiler::Compiler::new(dream_mir::backend::Target::native())
        .compile(&input.to_string_lossy().into_owned(), &ir.to_string_lossy())
        .unwrap();
    let diagnostic = dream::execution::native::compile_and_capture(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        ir.to_str().unwrap(),
        dream::driver::wasm_opt::OptLevel::O0,
    )
    .expect_err("all-strong cycles must remain allocated")
    .to_string();
    assert!(
        diagnostic.contains("guest leak check live=1"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("[dream] leak check: live=1"),
        "{diagnostic}"
    );
}
