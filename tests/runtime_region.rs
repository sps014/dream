#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn regions_chain_rewind_and_fall_back_independently_per_thread() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/native");
    let temp = tempfile::tempdir().expect("region test directory");
    let binary = temp.path().join("region");
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
        .arg(root.join("tests/runtime_region.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for region regressions");
    assert!(
        build.status.success(),
        "region harness failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(binary).output().expect("region harness");
    assert!(
        output.status.success(),
        "region harness failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"region stress passed\n");
}
