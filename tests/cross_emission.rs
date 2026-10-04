#![cfg(feature = "native")]

use object::Object;
use std::process::Command;

#[test]
fn emits_verified_linux_aarch64_object_without_linking() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("cross.ll");
    let result = Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["--object", "--target", "aarch64-unknown-linux-gnu", "-o"])
        .arg(&output)
        .arg("tests/cases/arithmetic.dream")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(output.with_extension("o")).unwrap();
    let object = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(object.format(), object::BinaryFormat::Elf);
    assert_eq!(object.architecture(), object::Architecture::Aarch64);
    assert!(object.is_little_endian());
    assert!(output.exists());
    assert!(!output.with_extension("bin").exists());
    let ir = std::fs::read_to_string(output).unwrap();
    assert!(ir.contains("aarch64-unknown-linux-gnu"));
}

#[test]
fn target_objects_cannot_be_run() {
    for subcommand in ["run", "test", "debug-adapter"] {
        let result = Command::new(env!("CARGO_BIN_EXE_dream"))
            .args([
                "--object",
                "--target",
                "aarch64-unknown-linux-gnu",
                subcommand,
                "tests/cases/arithmetic.dream",
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("require a linked host executable")
        );
    }
}

#[test]
fn foreign_tests_cannot_silently_run_for_the_host() {
    let triple = if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "x86_64-unknown-linux-gnu"
    } else {
        "aarch64-unknown-linux-gnu"
    };
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["--target", triple, "test", "tests/cases/arithmetic.dream"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("require a linked host executable"));
}
