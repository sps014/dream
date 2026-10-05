#![cfg(feature = "native")]

use std::fs;
use std::process::Command;

#[test]
fn verbose_metrics_cover_phases_without_changing_ir() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("sample.dream");
    let output = dir.path().join("out/sample.ll");
    fs::write(&source, "import system; fun identity<T>(x: T): T { return x; } fun main(): void { System.println(identity<int>(7)); }").unwrap();
    let compile = |verbose: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dream"));
        command
            .arg("--emit-llvm")
            .arg(&source)
            .arg("-o")
            .arg(&output);
        if verbose {
            command.arg("-v");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stderr).unwrap()
    };
    let quiet = compile(false);
    let ir = fs::read(output.with_extension("opt.ll")).unwrap();
    assert!(!quiet.contains("compile_phase"));
    assert!(!quiet.contains("peak_resident_bytes"));
    fs::remove_dir_all(output.parent().unwrap()).unwrap();
    let verbose = compile(true);
    for phase in [
        "parse",
        "sema",
        "monomorphization",
        "lowering",
        "module_passes",
        "per_function_passes",
        "ir_emission",
    ] {
        assert!(
            verbose.contains(&format!("phase=\"{phase}\"")),
            "missing {}: {}",
            phase,
            verbose
        );
    }
    for field in [
        "time.busy",
        "time.idle",
        "mir_pass",
        "compile_tool",
        "peak_resident_bytes",
    ] {
        assert!(verbose.contains(field), "missing {}: {}", field, verbose);
    }
    assert_eq!(ir, fs::read(output.with_extension("opt.ll")).unwrap());
}

#[test]
fn failed_parse_still_reports_its_duration_and_memory() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("broken.dream");
    fs::write(&source, "fun main(:").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_dream"))
        .arg("-v")
        .arg(&source)
        .arg("-o")
        .arg(dir.path().join("broken.ll"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("phase=\"parse\""));
    assert!(stderr.contains("time.busy"));
    let peak = stderr
        .split("peak_resident_bytes=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert!(peak > 0);
    assert!(!stderr.contains("phase=\"lowering\""));
}
