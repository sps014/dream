#![cfg(all(feature = "native", unix))]

use std::{path::Path, process::Command};

#[test]
fn empty_heap_reporting_skips_the_heap_walk_and_a_used_heap_reports_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = root.join("crates/dream-mir/src/runtime/c");
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("report");
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-std=gnu11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(runtime.join("core/include"))
        .arg("-I")
        .arg(runtime.join("sys/native/include"))
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
    for heap in [false, true] {
        let output = Command::new(&binary)
            .arg(if heap { "1" } else { "0" })
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let expected = if heap {
            "[dream] leak check: live=42 total_allocations=99 live_bytes=3000000000\n\
             [dream] leak by type: Fixture=42 (1344 B)\n"
        } else {
            "[dream] leak check: live=0 total_allocations=0 live_bytes=0\n\
             [dream] leak by type: (none)\n"
        };
        assert_eq!(String::from_utf8(output.stderr).unwrap(), expected);
    }
}

/// Drives the tally directly: more than the old 256-type list and per-type counts past
/// `INT32_MAX`, which is where the 32-bit histogram overflowed (2^31 blocks of 32 bytes is 64 GiB).
#[test]
fn leak_tally_counts_past_i32_and_never_drops_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = root.join("crates/dream-mir/src/runtime/c");
    let native = runtime.join("sys/native");
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("tally");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command
        .args(["-std=gnu11", "-O2", "-pthread", "-DDREAM_HEAP_COUNTERS=1"])
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
        "ownership.c",
        "heap_maps.c",
        "heap_debug.c",
        "publish.c",
        "region.c",
        "weak.c",
        "sync.c",
        "strings.c",
    ] {
        let path = dream_mir::runtime::native_runtime_units(
            &runtime,
            dream_mir::runtime::RuntimeNeed::CORE,
        )
        .into_iter()
        .find(|entry| entry.path.file_name().is_some_and(|name| name == unit))
        .unwrap()
        .path;
        command.arg(path);
    }
    let build = command
        .arg(root.join("tests/runtime_leak_tally.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(&binary).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|line| line.starts_with("[dream] leak by type:"))
        .unwrap_or_else(|| panic!("no tally line in {stderr}"));
    assert!(
        line.starts_with(
            "[dream] leak by type: object=3000000000 (96000000000 B) object=3000000000 (64000000000 B)"
        ),
        "{line}"
    );
    assert_eq!(line.matches(" (").count(), 17, "{line}");
    assert!(
        line.ends_with(" other=286 types (286 objects, 4576 B)"),
        "{line}"
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "tally passed\n");
}
