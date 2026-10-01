#![cfg(all(feature = "native", unix, target_pointer_width = "64"))]

use std::path::Path;
use std::process::Command;

#[test]
fn native_allocations_use_machine_width_without_committing_large_buffers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/native");
    let temp = tempfile::tempdir().expect("allocation test directory");
    let binary = temp.path().join("machine-size");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args([
        "-std=gnu11",
        "-O2",
        "-pthread",
        "-Wall",
        "-Wextra",
        "-Werror",
    ]);
    for unit in ["heap_maps.c", "publish.c", "region.c", "weak.c", "sync.c"] {
        command.arg(native.join(unit));
    }
    let build = command
        .arg(root.join("tests/runtime_machine_size.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for allocation regressions");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for mode in [
        "private",
        "shared",
        "realloc",
        "array",
        "from-bytes",
        "future",
        "string",
        "region",
    ] {
        let output = Command::new(&binary)
            .arg(mode)
            .output()
            .expect("allocation harness");
        assert!(output.status.success(), "{}: {:?}", mode, output);
    }
}
