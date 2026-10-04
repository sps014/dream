#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn foreign_callback_releases_keep_arc_on_the_owner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().expect("callback test directory");
    let binary = temp.path().join("callbacks");
    let build = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args([
            "-std=gnu11",
            "-O2",
            "-pthread",
            "-Wall",
            "-Wextra",
            "-Werror",
        ])
        .arg("-I")
        .arg(root.join("crates/dream-mir/src/runtime/c/core/include"))
        .arg(root.join("crates/dream-mir/src/runtime/c/sys/native/callback.c"))
        .arg(root.join("tests/runtime_callback.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for callback regressions");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(&binary)
        .output()
        .expect("callback stress harness");
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"callback foreign release stress passed\n");
    let output = Command::new(&binary)
        .arg("wrong-owner")
        .output()
        .expect("wrong-owner harness");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("callback owner panic"));
}
