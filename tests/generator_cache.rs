//! Generator caching end to end through the CLI (a subprocess, so the process-wide `GenStats`
//! counters of parallel tests never mix): fast-path skips, `@incremental` replays, relocation,
//! cold-vs-cached output parity, and `--capture` / `--replay`.

use std::path::{Path, PathBuf};
use std::process::Command;

const JSON_PROGRAM: &str = r#"import system;
import system.json;

@json
class Point {
    public x: int;
    public y: int;
    public constructor(x: int, y: int) { this.x = x; this.y = y; }
}

fun main(): void {
    System.println(Json.serialize(Point(1, 2)));
}
"#;

const PLAIN_JSON_IMPORT: &str = r#"import system;
import system.json;

fun main(): void {
    System.println("no derives");
}
"#;

fn project(tag: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dream_gencache_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.dream"), source).unwrap();
    dir
}

/// `dream -v --emit-llvm` with a fresh build-cache nonce; returns `(gen log lines, emitted IR)`.
fn compile(dir: &Path) -> (Vec<String>, String) {
    let out_ll = dir.join("out.ll");
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(dir)
        .env(
            "DREAM_BENCH_NONCE",
            format!("{:?}", std::time::Instant::now()),
        )
        .args(["-v", "--emit-llvm", "-o"])
        .arg(&out_ll)
        .arg("main.dream")
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "compile failed:\n{log}");
    let gen_lines = log
        .lines()
        .filter_map(|l| l.find("gen ").map(|i| l[i..].to_string()))
        .collect();
    (
        gen_lines,
        std::fs::read_to_string(out_ll.with_extension("opt.ll")).unwrap(),
    )
}

fn has(lines: &[String], want: &str) -> bool {
    lines.iter().any(|l| l == want)
}

fn llvm_available() -> bool {
    dream::execution::llvm::tools::resolve_llvm(&std::sync::Arc::new(
        dream::driver::toolchain::ToolchainConfig::default(),
    ))
    .is_ok()
}

#[test]
#[ignore = "builds the std json generator executable; cargo test --workspace -- --ignored"]
fn untriggered_generator_is_skipped_without_a_process() {
    if !llvm_available() {
        return;
    }
    let dir = project("skip", PLAIN_JSON_IMPORT);
    let (lines, _) = compile(&dir);
    assert!(has(&lines, "gen json_derive: skipped"), "{:?}", lines);
    assert!(
        !lines
            .iter()
            .any(|l| l.contains(": run ") || l.contains(": exe ")),
        "{:?}",
        lines
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "builds the std json generator executable; cargo test --workspace -- --ignored"]
fn unchanged_incremental_input_replays_and_survives_relocation() {
    if !llvm_available() {
        return;
    }
    let dir = project("replay", JSON_PROGRAM);
    let (_, first_ir) = compile(&dir);
    let (warm, warm_ir) = compile(&dir);
    assert!(has(&warm, "gen json_derive: result hit"), "{:?}", warm);
    assert!(
        !warm
            .iter()
            .any(|l| l.contains(": run ") || l.contains("exe miss")),
        "{:?}",
        warm
    );
    assert_eq!(
        first_ir, warm_ir,
        "a replayed result must emit the same IR as a fresh run"
    );

    let moved = project("replay_moved", JSON_PROGRAM);
    let (relocated, _) = compile(&moved);
    assert!(
        has(&relocated, "gen json_derive: result hit"),
        "{:?}",
        relocated
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(moved);
}

#[test]
#[ignore = "builds the std json generator executable; cargo test --workspace -- --ignored"]
fn capture_then_replay_reproduces_the_result() {
    if !llvm_available() {
        return;
    }
    let dir = project("capture", JSON_PROGRAM);
    let capture_dir = dir.join("cap");
    let bin = env!("CARGO_BIN_EXE_dream");
    let captured = Command::new(bin)
        .current_dir(&dir)
        .args(["generate", "main.dream", "--capture", "json_derive", "-o"])
        .arg(&capture_dir)
        .output()
        .unwrap();
    assert!(
        captured.status.success(),
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    for file in ["capture.json", "snapshot.json", "result.json"] {
        assert!(capture_dir.join(file).is_file(), "missing {}", file);
    }
    let replayed = Command::new(bin)
        .args(["generate", "--replay"])
        .arg(&capture_dir)
        .output()
        .unwrap();
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let _ = std::fs::remove_dir_all(dir);
}
