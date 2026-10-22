#![cfg(feature = "native")]
use crate::common::http_fixture as fixture;
use std::{
    process::{Command, Output},
    sync::{Arc, atomic::Ordering},
};

fn successful(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn download_cli_runs_on_native() {
    let server = fixture::HttpFixture::start();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = root.join("download.dream");
    std::fs::write(&source, include_str!("../sample/cli_download/main.dream")).unwrap();
    let compiler = env!("CARGO_BIN_EXE_dream");
    let output = root.join("native.bin");
    successful(
        Command::new(compiler)
            .arg("-o")
            .arg(output.with_extension("ll"))
            .arg(&source)
            .output()
            .unwrap(),
    );
    let invoke = |args: &[&str]| {
        let mut command = dream::execution::native::native_command(
            &Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
            &output,
            &output.with_extension("ll").to_string_lossy(),
            None,
        )
        .unwrap();
        let output = command
            .args(args)
            .env("DREAM_LEAK_CHECK", "1")
            .current_dir(root)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let live = stderr
            .lines()
            .find_map(|line| line.strip_prefix("[dream] leak check: live="))
            .and_then(|tail| tail.split_whitespace().next());
        assert_eq!(live, Some("0"), "CLI left live resources: {output:?}");
        output
    };
    let destination = root.join("native output 日本語.bin");
    let url = format!("{}/redirect", server.url);
    assert_eq!(
        successful(invoke(&[&url, "-o", destination.to_str().unwrap()])),
        ""
    );
    assert_eq!(std::fs::read(&destination).unwrap(), b"firstsecond");
    let preserved = root.join("preserved.bin");
    std::fs::write(&preserved, b"preserve existing contents").unwrap();
    let rejected = invoke(&[
        &format!("{}/echo", server.url),
        "-o",
        preserved.to_str().unwrap(),
    ]);
    assert_eq!(rejected.status.code(), Some(1));
    assert_eq!(
        std::fs::read(&preserved).unwrap(),
        b"preserve existing contents"
    );
    let timeout = invoke(&[
        &format!("{}/slow", server.url),
        "--timeout=20",
        "-o",
        "timed-out.bin",
    ]);
    assert_eq!(timeout.status.code(), Some(1));
    assert!(!root.join("timed-out.bin").exists());
    let negative = invoke(&[&url, "--timeout=-1"]);
    assert_eq!(negative.status.code(), Some(2));
    let help = successful(invoke(&["--help"]));
    assert!(help.contains("Usage: download"));
    assert_eq!(successful(invoke(&["--version"])), "download 1.0.0\n");
    let invalid = invoke(&[&url, "--timeout", "oops"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("integer"));
    let failure = invoke(&[&format!("{}/echo", server.url), "-o", "not-created.bin"]);
    assert_eq!(failure.status.code(), Some(1));
    assert!(!root.join("not-created.bin").exists());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("HTTP status 404"));
    assert!(server.accepted.load(Ordering::Relaxed) > 0);
}
