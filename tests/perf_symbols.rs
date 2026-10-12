#![cfg(feature = "native")]

use dream::driver::compiler::{BuildOutcome, Compiler};
use dream_mir::backend::Target;

fn instructions(ir: &str) -> Vec<String> {
    let mut in_function = false;
    ir.lines()
        .filter_map(|line| {
            if line.starts_with("define ") {
                in_function = true;
            }
            if !in_function {
                return None;
            }
            let line = line.split(", !dbg !").next().unwrap();
            let line = match line.split_once(" !dbg !") {
                Some((definition, _)) => format!("{definition} {{"),
                None => line.to_string(),
            };
            if line == "}" {
                in_function = false;
            }
            Some(line)
        })
        .collect()
}

#[test]
fn profiling_preserves_release_instructions_including_async_frames() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(
        &source,
        r#"
import system;
class Pair {
    public x: int;
    public constructor(x: int) { this.x = x; }
}
fun twice(x: int): int { return x + x; }
fun compute(n: int): int {
    let p = Pair(n);
    return twice(p.x);
}
async fun work(n: int): int {
    let before = twice(n);
    Time.sleep(1).await;
    let after = compute(before);
    Time.sleep(1).await;
    return twice(after);
}
async fun main(): void { System.println(work(7).await); }
"#,
    )
    .unwrap();
    let compile = |perf| {
        Compiler::new(Target::native())
            .with_release(true)
            .with_perf_symbols(perf)
            .compile(&source.display().to_string(), out.to_str().unwrap())
            .unwrap();
        std::fs::read_to_string(&out).unwrap()
    };
    let release = compile(false);
    let profiling = compile(true);
    assert!(!release.contains("!DICompileUnit"));
    assert!(profiling.contains("!DICompileUnit"));
    assert!(profiling.contains("isOptimized: true"));
    assert!(!profiling.contains("#dbg_declare"));
    let ordinary = instructions(&release);
    let symbolized = instructions(&profiling);
    assert_eq!(ordinary.len(), symbolized.len());
    for (i, (ordinary, symbolized)) in ordinary.iter().zip(&symbolized).enumerate() {
        assert_eq!(ordinary, symbolized, "instruction {i}");
    }
    assert_eq!(
        profiling,
        compile(true),
        "profiling emission must be deterministic"
    );
}

#[test]
fn profiling_and_debugger_settings_have_distinct_cache_identities() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(&source, "fun main(): void {}\n").unwrap();
    let compile = |perf, debug| {
        Compiler::new(Target::native())
            .with_release(true)
            .with_perf_symbols(perf)
            .with_debug_info(debug)
            .with_build_cache(Some("same-link-settings".into()))
            .compile(&source.display().to_string(), out.to_str().unwrap())
            .unwrap()
    };
    for (perf, debug) in [(false, false), (true, false), (false, true)] {
        let BuildOutcome::Built(Some(stamp)) = compile(perf, debug) else {
            panic!("changed symbol policy must rebuild");
        };
        stamp.store(std::slice::from_ref(&out));
        assert!(matches!(compile(perf, debug), BuildOutcome::Cached(_)));
    }
}
