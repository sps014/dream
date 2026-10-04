#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn core_terminal_paths_use_only_platform_abort() {
    let native = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/dream-mir/src/runtime/c/core");
    for entry in std::fs::read_dir(native).expect("native runtime directory") {
        let path = entry.expect("runtime file").path();
        if path.extension().is_some_and(|ext| ext == "c")
            && path.file_name().is_some_and(|name| name != "panic.c")
        {
            let source = std::fs::read_to_string(&path).expect("runtime source");
            assert!(
                !source
                    .replace("dream_platform_abort(", "")
                    .replace("->abort(", "")
                    .contains("abort("),
                "direct abort in {}",
                path.display()
            );
        }
    }
}

#[test]
fn fatal_runtime_paths_report_without_allocating_or_deadlocking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/sys/native");
    let temp = tempfile::tempdir().expect("panic test directory");
    let binary = temp.path().join("panic");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args(["-std=gnu11", "-O2", "-pthread"]);
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
        .arg(root.join("tests/runtime_panic.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for panic regressions");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for (mode, reason) in [
        (
            "private-size",
            "allocation size exceeds the supported limit",
        ),
        ("shared-size", "allocation size exceeds the supported limit"),
        (
            "realloc-size",
            "allocation size exceeds the supported limit",
        ),
        ("array-size", "array size exceeds the supported limit"),
        ("string-size", "string size exceeds the supported limit"),
        ("string-count", "string length exceeds the supported limit"),
        (
            "string-byte-count",
            "string byte count exceeds the int range",
        ),
        (
            "js",
            "JavaScript calls are unavailable in the native runtime",
        ),
        ("counters", "out of memory registering heap counters"),
        ("private-map", "out of memory mapping the private heap"),
        ("shared-map", "out of memory mapping the shared heap"),
    ] {
        let output = Command::new(&binary)
            .arg(mode)
            .output()
            .expect("panic harness");
        assert_eq!(output.status.code(), Some(86), "{mode}: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).trim(),
            format!("panic: {reason}")
        );
    }
}
