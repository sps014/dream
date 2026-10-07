#![cfg(all(feature = "native", unix))]

use std::path::Path;
use std::process::Command;

#[test]
fn publishes_cyclic_diamond_and_million_node_task_graphs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native = root.join("crates/dream-mir/src/runtime/c/sys/native");
    let temp = tempfile::tempdir().expect("publication test directory");
    let binary = temp.path().join("publish");
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
        "heap.c",
        "cycles.c",
        "heap_maps.c",
        "heap_debug.c",
        "publish.c",
        "region.c",
        "weak.c",
        "sync.c",
        "strings.c",
        "worker.c",
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
        .arg(root.join("tests/runtime_publish.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C compiler for publication regressions");
    assert!(
        build.status.success(),
        "publication harness failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(binary).output().expect("publication harness");
    assert!(
        output.status.success(),
        "publication harness failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"publication stress passed\n");
}
