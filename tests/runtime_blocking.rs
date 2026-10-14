#![cfg(feature = "native")]

use std::{path::Path, process::Command};

#[test]
fn blocking_executor_bounds_workers_queue_and_preserves_rejected_contexts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = root.join("crates/dream-mir/src/runtime/c");
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join(if cfg!(windows) {
        "blocking.exe"
    } else {
        "blocking"
    });
    let mut command = if cfg!(windows) {
        let config = std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default());
        let tools = dream::execution::llvm::resolve_llvm(&config).unwrap();
        Command::new(tools.clang().unwrap())
    } else {
        let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
        command.arg("-pthread");
        command
    };
    command.args(["-std=gnu11", "-O2", "-Wall", "-Werror"]);
    if let Ok(sanitizer) = std::env::var("DREAM_WORKER_SANITIZER") {
        assert!(matches!(sanitizer.as_str(), "thread" | "address,undefined"));
        command.arg(format!("-fsanitize={sanitizer}")).arg("-g");
    }
    let build = command
        .arg("-I")
        .arg(runtime.join("core/include"))
        .arg("-I")
        .arg(runtime.join("sys/native/include"))
        .arg(root.join("tests/runtime_blocking.c"))
        .arg(runtime.join("sys/native/blocking.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(binary).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "bounded blocking executor passed\n"
    );
}
