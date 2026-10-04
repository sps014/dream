#![cfg(all(feature = "native", unix, target_pointer_width = "64"))]

use std::path::Path;
use std::process::Command;

#[test]
fn native_allocations_use_machine_width_without_committing_large_buffers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/sys/native");
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
    for unit in ["heap_maps.c", "publish.c", "region.c", "weak.c", "sync.c"] {
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
