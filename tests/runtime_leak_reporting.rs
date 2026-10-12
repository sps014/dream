#![cfg(all(feature = "native", unix))]

use std::{path::Path, process::Command};

#[test]
fn empty_heap_reporting_keeps_debug_and_environment_behavior_without_reading_heap_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = root.join("crates/dream-mir/src/runtime/c");
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("report");
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-std=gnu11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(runtime.join("core/include"))
        .arg(root.join("tests/runtime_leak_reporting.c"))
        .arg(runtime.join("sys/native/leak_report.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (heap, always, environment) in [
        (false, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, false),
    ] {
        let mut command = Command::new(&binary);
        command
            .arg(if heap { "1" } else { "0" })
            .arg(if always { "1" } else { "0" });
        command.env_remove("DREAM_DEBUG_LEAKS");
        if environment {
            command.env("DREAM_DEBUG_LEAKS", "1");
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let expected = if !always && !environment {
            ""
        } else if heap {
            "[dream] leak check: live=42 total_allocations=99\n[dream] leak by type: Fixture=42\n"
        } else {
            "[dream] leak check: live=0 total_allocations=0\n[dream] leak by type: (none)\n"
        };
        assert_eq!(String::from_utf8(output.stderr).unwrap(), expected);
    }
}
