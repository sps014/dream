#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn native_runtime_has_one_terminal_abort() {
    let native =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/dream-mir/src/runtime/c/native");
    for entry in std::fs::read_dir(native).expect("native runtime directory") {
        let path = entry.expect("runtime file").path();
        if path.extension().is_some_and(|ext| ext == "c")
            && path.file_name().is_some_and(|name| name != "panic.c")
        {
            let source = std::fs::read_to_string(&path).expect("runtime source");
            assert!(
                !source.contains("abort("),
                "direct abort in {}",
                path.display()
            );
        }
    }
}

#[test]
fn fatal_runtime_paths_report_without_allocating_or_deadlocking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/native");
    let temp = tempfile::tempdir().expect("panic test directory");
    let binary = temp.path().join("panic");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args(["-std=gnu11", "-O2", "-pthread"]);
    for unit in [
        "heap_maps.c",
        "publish.c",
        "region.c",
        "weak.c",
        "sync.c",
        "strings.c",
    ] {
        command.arg(native.join(unit));
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
