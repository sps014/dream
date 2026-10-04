#![cfg(feature = "native")]

use std::path::Path;
use std::process::Command;

#[test]
fn core_runs_and_reports_exhaustion_through_the_injected_platform() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let core = root.join("crates/dream-mir/src/runtime/c/core");
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join(if cfg!(windows) {
        "platform.exe"
    } else {
        "platform"
    });
    let config = std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default());
    let tools = dream::execution::llvm::resolve_llvm(&config).unwrap();
    let mut command = Command::new(tools.clang().unwrap());
    command
        .args(["-std=gnu11", "-O1"])
        .arg("-I")
        .arg(core.join("include"));
    let mut sources = std::fs::read_dir(&core)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
        .collect::<Vec<_>>();
    sources.sort();
    let build = command
        .args(sources)
        .arg(root.join("tests/runtime_platform.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(&binary).arg("normal").output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("injected platform passed"));
    let nul = Command::new(&binary).arg("nul").output().unwrap();
    assert!(nul.status.success());
    assert_eq!(nul.stderr, b"a\0b");
    let long = Command::new(&binary).arg("long").output().unwrap();
    assert_eq!(long.status.code(), Some(86));
    assert_eq!(
        long.stderr.iter().filter(|byte| **byte == b'x').count(),
        3000
    );
    for (mode, message) in [
        ("counters", "out of memory registering heap counters"),
        ("mapping", "out of memory mapping the private heap"),
        ("index", "out of memory indexing the Dream heap"),
        ("panic", "unicode: \u{1f600}\n  at host/library.dream:17"),
        ("hook", "HOOK"),
    ] {
        let output = Command::new(&binary).arg(mode).output().unwrap();
        assert_eq!(output.status.code(), Some(86), "{mode}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
        assert!(
            stderr.contains(message) && stderr.contains("PLATFORM_ABORT"),
            "{}: {}",
            mode,
            stderr
        );
    }
}
