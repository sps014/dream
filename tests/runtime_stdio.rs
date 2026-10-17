#![cfg(all(feature = "native", unix))]

use std::{path::Path, process::Command};

#[test]
fn chunked_output_preserves_streams_unicode_and_c_string_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = root.join("crates/dream-mir/src/runtime/c");
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("stdio");
    let build = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-std=gnu11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(runtime.join("core/include"))
        .arg("-I")
        .arg(runtime.join("sys/native/include"))
        .arg("-I")
        .arg(runtime.join("include"))
        .arg(runtime.join("core/utf8.c"))
        .arg(runtime.join("sys/native/stdio.c"))
        .arg(root.join("tests/runtime_stdio.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let mut expected = vec![b'x'; 1022];
    expected.extend_from_slice("😀".as_bytes());
    expected.extend_from_slice(&[0xed, 0xa0, 0x80, b'y', 0xed, 0xb0, 0x80]);
    expected.extend_from_slice("é".as_bytes());
    for mode in ["stdout", "stderr"] {
        let output = Command::new(&binary).arg(mode).output().unwrap();
        assert!(output.status.success());
        if mode == "stdout" {
            assert_eq!(output.stdout, expected);
            assert!(output.stderr.is_empty());
        } else {
            assert_eq!(output.stderr, expected);
            assert!(output.stdout.is_empty());
        }
    }
}
