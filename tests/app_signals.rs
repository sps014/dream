#![cfg(all(feature = "native", unix))]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn build(directory: &Path) -> PathBuf {
    let source = crate::common::compiler_fixture("app_signals");
    let binary = directory.join("app_signals.bin");
    let built = Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["build", "-o"])
        .arg(binary.with_extension("ll"))
        .arg(source)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    binary
}

/// Runs the fixture, sends `signals` in order (each after the line it names), and returns the
/// exit code and every stdout line.
fn run_with_signals(binary: &Path, signals: &[(&str, &str)]) -> (Option<i32>, Vec<String>) {
    let mut child = Command::new(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id().to_string();
    let mut lines = Vec::new();
    let mut pending = signals.iter();
    let mut next = pending.next();
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.unwrap();
        if let Some((after, signal)) = next
            && line == *after
        {
            let sent = Command::new("kill")
                .args([&format!("-{signal}"), pid.as_str()])
                .status()
                .unwrap();
            assert!(sent.success());
            next = pending.next();
        }
        lines.push(line);
    }
    (child.wait().unwrap().code(), lines)
}

#[test]
fn interrupt_and_terminate_run_cleanups_and_report_signal_exit_codes() {
    let directory = tempfile::tempdir().unwrap();
    let binary = build(directory.path());
    let expected = ["ready", "requested", "cleanup start", "cleanup done"];

    let (code, lines) = run_with_signals(&binary, &[("ready", "INT")]);
    assert_eq!(code, Some(130));
    assert_eq!(lines, expected);

    let (code, lines) = run_with_signals(&binary, &[("ready", "TERM")]);
    assert_eq!(code, Some(143));
    assert_eq!(lines, expected);
}

#[test]
fn second_interrupt_exits_without_waiting_for_cleanups() {
    let directory = tempfile::tempdir().unwrap();
    let binary = build(directory.path());
    let (code, lines) = run_with_signals(&binary, &[("ready", "INT"), ("cleanup start", "INT")]);
    assert_eq!(code, Some(130));
    assert_eq!(lines, ["ready", "requested", "cleanup start"]);
}
