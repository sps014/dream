#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn localized_cycles_are_reclaimed_in_allocation_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/sys/native");
    let temp = tempfile::tempdir().expect("cycle test directory");
    let binary = temp.path().join("cycles");
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
        "cycles.c",
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
        .arg(root.join("tests/runtime_cycles.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for cycle regressions");
    assert!(
        build.status.success(),
        "cycle harness failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(binary).output().expect("cycle harness");
    assert!(
        output.status.success(),
        "cycle harness failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"cycle trial deletion passed\n");
}
