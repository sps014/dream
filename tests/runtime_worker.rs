#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn worker_registry_grows_and_cleans_up_failed_starts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/native");
    let temp = tempfile::tempdir().expect("worker test directory");
    let binary = temp.path().join("worker");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args(["-std=gnu11", "-O2", "-pthread"]);
    for unit in [
        "heap.c",
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
        .arg(root.join("tests/runtime_worker.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for worker regressions");
    assert!(
        build.status.success(),
        "worker harness failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(&binary).output().expect("worker stress harness");
    assert!(
        output.status.success(),
        "worker stress failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"worker registry stress passed\n");
    for (mode, message) in [
        ("start-failure", "panic: could not start a worker thread"),
        ("id-exhaustion", "panic: worker ID space exhausted"),
    ] {
        let output = Command::new(&binary)
            .arg(mode)
            .output()
            .expect("worker failure harness");
        assert_eq!(output.status.code(), Some(86), "{mode}: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), message);
    }
}
