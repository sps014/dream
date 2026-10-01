#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn lock_registry_reclaims_reused_addresses_and_checks_ownership() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/native");
    let temp = tempfile::tempdir().expect("lock test directory");
    let binary = temp.path().join("sync");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args(["-std=gnu11", "-O2", "-pthread"]);
    for unit in [
        "heap.c",
        "heap_maps.c",
        "publish.c",
        "region.c",
        "weak.c",
        "strings.c",
    ] {
        command.arg(native.join(unit));
    }
    let build = command
        .arg(root.join("tests/runtime_sync.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for lock regressions");
    assert!(
        build.status.success(),
        "lock harness failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(&binary).output().expect("lock stress harness");
    assert!(
        output.status.success(),
        "lock stress failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"lock registry stress passed\n");
    for (mode, message) in [
        (
            "wrong-thread",
            "panic: lock release requires the owning thread",
        ),
        ("unheld", "panic: lock release requires the owning thread"),
        (
            "waiting-destroy",
            "panic: destroying a lock with waiting threads",
        ),
    ] {
        let output = Command::new(&binary)
            .arg(mode)
            .output()
            .expect("lock failure harness");
        assert_eq!(output.status.code(), Some(86), "{mode}: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), message);
    }
}
