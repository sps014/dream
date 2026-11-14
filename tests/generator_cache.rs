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
    compile_with(dir, &[])
}

fn compile_with(dir: &Path, env: &[(&str, &str)]) -> (Vec<String>, String) {
    let out_ll = dir.join("out.ll");
    let mut command = Command::new(env!("CARGO_BIN_EXE_dream"));
    command
        .current_dir(dir)
        .env(
            "DREAM_BENCH_NONCE",
            format!("{:?}", std::time::Instant::now()),
        )
        .args(["-v", "--emit-llvm", "-o"])
        .arg(&out_ll)
        .arg("main.dream");
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().unwrap();
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

fn generated_sources(dir: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "dream")
                && path != dir.join("main.dream")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                files.push((path.display().to_string(), text));
            }
        }
    }
    files.sort();
    files
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
    assert!(
        has(&lines, "gen system.json.derive::json_derive: skipped"),
        "{:?}",
        lines
    );
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
    assert!(
        has(&warm, "gen system.json.derive::json_derive: result hit"),
        "{:?}",
        warm
    );
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
        has(
            &relocated,
            "gen system.json.derive::json_derive: result hit"
        ),
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

#[test]
#[ignore = "builds the std json generator executable; cargo test --workspace -- --ignored"]
fn program_cache_matches_a_generation_that_skips_the_cache() {
    if !llvm_available() {
        return;
    }
    let source = JSON_PROGRAM.replace("Point(1, 2)", "Point(7, 8)");
    let dir = project("equiv", &source);
    let (first_log, first_ir) = compile(&dir);
    assert!(
        first_log.iter().any(|line| line.contains(": run ")),
        "a new program must run the generator: {first_log:?}"
    );
    let (hit_log, hit_ir) = compile(&dir);
    assert!(has(&hit_log, "gen program: cache hit"), "{hit_log:?}");
    assert_eq!(first_ir, hit_ir, "a cache hit must emit the same IR");
    let _ = std::fs::remove_dir_all(dir);
}

const ROUTE_PROGRAM: &str = r#"import system;
import system.encoding;
import system.webapi;

@router("")
class Routes {
    @get("/health")
    public static fun health(): string {
        return Encoding.hex_encode(("ok").to_utf8());
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
}
"#;

#[test]
#[ignore = "builds the std webapi generator executable; cargo test --workspace -- --ignored"]
fn program_cache_hit_keeps_access_to_private_referenced_functions() {
    if !llvm_available() {
        return;
    }
    let dir = project("access", ROUTE_PROGRAM);
    let (_, first_ir) = compile(&dir);
    let (hit_log, hit_ir) = compile(&dir);
    assert!(has(&hit_log, "gen program: cache hit"), "{hit_log:?}");
    assert_eq!(first_ir, hit_ir, "a cache hit must emit the same IR");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "builds the std json generator executable; cargo test --workspace -- --ignored"]
fn unrelated_function_edit_reuses_the_program_cache() {
    if !llvm_available() {
        return;
    }
    let source = format!("{JSON_PROGRAM}\nfun unused(): int {{ return 1; }}\n");
    let dir = project("unrelated", &source);
    compile(&dir);
    std::fs::write(
        dir.join("main.dream"),
        source.replace("return 1", "return 2"),
    )
    .unwrap();
    let (warm, _) = compile(&dir);
    assert!(
        has(&warm, "gen system.json.derive::json_derive: result hit"),
        "{warm:?}"
    );
    assert!(!warm.iter().any(|line| line.contains(": run ")), "{warm:?}");
    let cached = generated_sources(&dir);
    compile_with(&dir, &[("DREAM_GENERATOR_CACHE", "0")]);
    assert_eq!(cached, generated_sources(&dir));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "builds the std json generator executable; cargo test --workspace -- --ignored"]
fn editing_a_trigger_input_misses_the_program_cache() {
    if !llvm_available() {
        return;
    }
    let dir = project("miss", JSON_PROGRAM);
    compile(&dir);
    let edited = JSON_PROGRAM
        .replace("public y: int;", "public renamed: int;")
        .replace(
            "constructor(x: int, y: int)",
            "constructor(x: int, renamed: int)",
        )
        .replace("this.y = y", "this.renamed = renamed");
    std::fs::write(dir.join("main.dream"), edited).unwrap();
    let (log, _) = compile(&dir);
    assert!(
        log.iter().any(|line| line.contains(": run ")),
        "an attributed field edit must regenerate: {log:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}
