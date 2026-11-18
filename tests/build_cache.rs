#![cfg(feature = "native")]

use dream::driver::compiler::{BuildOutcome, Compiler};
use dream_mir::backend::Target;
use std::path::{Path, PathBuf};

fn build(source: &Path, out: &Path, link_key: &str) -> BuildOutcome {
    Compiler::new(Target::native())
        .with_build_cache(Some(link_key.to_string()))
        .compile(&source.display().to_string(), &out.display().to_string())
        .unwrap()
}

fn record(outcome: BuildOutcome, out: &Path) {
    match outcome {
        BuildOutcome::Built {
            stamp: Some(stamp), ..
        } => stamp.store(&[out.to_path_buf()]),
        BuildOutcome::Built { stamp: None, .. } => {
            panic!("diagnostic-free build should be cacheable")
        }
        BuildOutcome::Cached(_) => panic!("expected a fresh build"),
    }
}

fn cached(outcome: BuildOutcome) -> Option<Vec<PathBuf>> {
    match outcome {
        BuildOutcome::Cached(artifacts) => Some(artifacts),
        BuildOutcome::Built { .. } => None,
    }
}

#[test]
fn unchanged_inputs_reuse_the_recorded_build_and_any_change_rebuilds() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.dream");
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(&helper, "public fun answer(): int { return 42; }\n").unwrap();
    std::fs::write(
        &source,
        "import system;\nimport helper;\nfun main(): void { System.println(answer()); }\n",
    )
    .unwrap();

    record(build(&source, &out, "O0"), &out);
    assert_eq!(cached(build(&source, &out, "O0")), Some(vec![out.clone()]));

    assert!(cached(build(&source, &out, "O2")).is_none());

    std::fs::write(&helper, "public fun answer(): int { return 7; }\n").unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());

    std::fs::write(&out, "tampered").unwrap();
    assert!(cached(build(&source, &out, "O0")).is_none());
}

#[test]
fn aliased_imports_reuse_builds_and_invalidate_changed_bindings_and_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.dream");
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(
        &helper,
        "module helpers; public fun answer(): int { return 42; }",
    )
    .unwrap();
    let program = "import system; import helper; import helpers.answer as chosen; fun main(): void { System.println(chosen()); }";
    std::fs::write(&source, program).unwrap();

    record(build(&source, &out, "O0"), &out);
    let modified = std::fs::metadata(&out).unwrap().modified().unwrap();
    assert_eq!(cached(build(&source, &out, "O0")), Some(vec![out.clone()]));
    assert_eq!(
        std::fs::metadata(&out).unwrap().modified().unwrap(),
        modified
    );

    std::fs::write(
        &helper,
        "module helpers; public fun answer(): int { return 7; }",
    )
    .unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());

    std::fs::write(&source, program.replace("chosen", "renamed")).unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());

    std::fs::write(
        &source,
        program.replace("helpers.answer", "helpers.missing"),
    )
    .unwrap();
    let result = Compiler::new(Target::native())
        .with_build_cache(Some("O0".into()))
        .compile(&source.display().to_string(), &out.display().to_string());
    assert!(
        result.is_err(),
        "a stale build must not hide an invalid alias"
    );
}

#[test]
fn aliased_types_with_incremental_generators_reuse_builds() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(&source, "module app; import system; import system.json; import app.Point as P; @json public class Point { public x: int; public constructor(x: int) { this.x = x; } } fun main(): void { System.println(Json.serialize(P(1))); }").unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());
}

#[test]
fn cli_reports_cached_builds_without_rewriting_the_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.o");
    std::fs::write(&source, "fun main(): void {}\n").unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_dream"))
            .arg("--object")
            .arg("--output")
            .arg(&out)
            .arg(&source)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let modified = std::fs::metadata(&out).unwrap().modified().unwrap();
    let second = run();
    let log = String::from_utf8_lossy(&second.stderr);
    assert!(second.status.success(), "{log}");
    assert!(log.contains("Finished cached in"), "{log}");
    assert_eq!(
        std::fs::metadata(&out).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn builds_with_warnings_are_cached_and_replay_their_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.o");
    std::fs::write(
        &source,
        "import system;\nfun main(): void { let unused = 1; System.println(\"w\"); }\n",
    )
    .unwrap();
    let run = || {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_dream"))
            .arg("--object")
            .arg("--output")
            .arg(&out)
            .arg(&source)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(output.status.success(), "{log}");
        log
    };
    let first = run();
    assert!(first.contains("unused variable 'unused'"), "{first}");
    let second = run();
    assert!(second.contains("Finished cached in"), "{second}");
    assert!(second.contains("unused variable 'unused'"), "{second}");
}

#[test]
fn incremental_generated_sources_allow_early_lookup_and_detect_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(&source, "import system; import system.json;\n@json class Point { public x: int; public constructor(x: int) { this.x = x; } }\nfun main(): void { System.println(Json.serialize(Point(1))); }").unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());
    let stamp = out.with_extension("dream-cache");
    std::fs::write(&stamp, b"{partial").unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());
}

#[test]
fn cache_detects_new_local_resolution_candidates_and_manifest_changes() {
    let dir = tempfile::tempdir().unwrap();
    let packages = dir.path().join("dream_packages/helper/src");
    std::fs::create_dir_all(&packages).unwrap();
    std::fs::write(
        packages.join("helper.dream"),
        "public fun answer(): int { return 42; }",
    )
    .unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(
        &source,
        "import system; import helper; fun main(): void { System.println(answer()); }",
    )
    .unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());
    std::fs::write(
        dir.path().join("helper.dream"),
        "public fun answer(): int { return 7; }",
    )
    .unwrap();
    record(build(&source, &out, "O0"), &out);
    assert!(cached(build(&source, &out, "O0")).is_some());
    std::fs::write(dir.path().join("dream.lock"), "changed resolution").unwrap();
    assert!(cached(build(&source, &out, "O0")).is_none());
}

#[test]
fn concurrent_cli_requests_do_not_remove_each_others_llvm_input() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("main.dream");
    let output = temp.path().join("program.o");
    std::fs::write(&source, "fun main(): void {}\n").unwrap();
    let mut children = Vec::new();
    for debug in [false, true, false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_dream"));
        command
            .arg("--object")
            .arg("--output")
            .arg(&output)
            .arg(&source);
        if debug {
            command.arg("--debug-info");
        }
        children.push(
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert!(output.is_file());
}

#[test]
fn native_debug_reuses_unchanged_codegen_units_and_matches_a_clean_link() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = root.join("main.dream");
    let dependency = root.join("numbers.dream");
    let importer = root.join("caller.dream");
    let ir = root.join("main.ll");
    let bin = dream::driver::output::OutputKind::Executable
        .artifact_path(&ir, &dream_abi::target::TargetSpec::host());
    let cache = root.join("cache/native-rt/program-objects");
    let body_cache = root.join("cache/body-checks");
    std::fs::write(
        &source,
        "import system; import caller; import numbers; fun main(): void { System.println(describe(3)); System.println(compute() + (int)answer()); }",
    )
    .unwrap();
    std::fs::write(
        &importer,
        r#"module caller;
import system;
import numbers;

public interface Shape {
    fun area(): int;
}

public class Square : Shape {
    public side: int;

    public constructor(side: int) {
        this.side = side;
    }

    public fun area(): int {
        return this.side * this.side;
    }

    public fun grow(by: int): Square {
        return Square(this.side + by);
    }
}

let scale: int = 2;

fun pick<T>(a: T, b: T, first: bool): T {
    if (first) {
        return a;
    }
    return b;
}

fun total(shapes: List<Shape>): int {
    let sum = 0;
    for (let shape in shapes) {
        sum += shape.area();
    }
    return sum;
}

public fun compute(): int {
    let shapes = List<Shape>();
    shapes.push(Square(2));
    shapes.push(Square(3).grow(1));
    return total(shapes) * scale + pick<int>(1, 2, true);
}

public fun describe(n: int): string {
    let parts = List<string>();
    for (let i = 0; i < n; i += 1) {
        parts.push("x" + i.to_string());
    }
    return parts.join(",") + ":" + pick<string>("a", "b", false);
}
"#,
    )
    .unwrap();
    let write_dependency = |ret, value| {
        std::fs::write(
            &dependency,
            format!("module numbers; public fun answer(): {ret} {{ return {value}; }}"),
        )
        .unwrap();
    };
    let config = dream::driver::toolchain::ToolchainConfig::default();
    let tool = |name| {
        config
            .locate_tool(dream_abi::toolchain::compiler_tool(name).unwrap())
            .unwrap()
            .root
    };
    let llvm = tool("llvm");
    let mold = (!cfg!(windows)).then(|| tool("mold"));
    let link = |expected: &str, reused_modules: usize, reused_functions: usize| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_dream"));
        command
            .current_dir(root)
            .env("DREAM_PREFIX", root)
            .env("DREAM_LLVM", &llvm);
        if let Some(mold) = &mold {
            command.env("DREAM_MOLD", mold);
        }
        let result = command
            .args([
                "-v",
                "-o",
                ir.to_str().unwrap(),
                "run",
                source.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stdout)
                .lines()
                .any(|line| line == expected),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        let trace = format!(
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            trace.contains(&format!(
                "reusing syntax-only body checks modules={reused_modules}"
            )),
            "{trace}"
        );
        assert!(
            trace.contains(&format!("reused typed HIR functions={reused_functions}")),
            "{trace}"
        );
    };
    let objects = || {
        std::fs::read_dir(&cache)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                entry
                    .path()
                    .join("unit.o")
                    .is_file()
                    .then(|| entry.file_name())
            })
            .collect::<std::collections::BTreeSet<_>>()
    };

    write_dependency("int", "42");
    link("83", 0, 0);
    let cold = objects();
    assert_eq!(cold.len(), 3);
    write_dependency("int", "43");
    link("84", 9, 29);
    let edited = objects();
    assert_eq!(edited.difference(&cold).count(), 1);
    let incremental = std::fs::read(&bin).unwrap();
    let saved = root.join("saved-body-checks");
    std::fs::rename(&body_cache, &saved).unwrap();
    std::fs::remove_file(&bin).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();
    link("84", 0, 0);
    assert_eq!(std::fs::read(&bin).unwrap(), incremental);
    std::fs::remove_dir_all(&body_cache).unwrap();
    std::fs::rename(saved, &body_cache).unwrap();

    write_dependency("long", "44L");
    link("85", 7, 23);
    let interface_edit = objects();
    assert!(interface_edit.difference(&edited).count() >= 2);
    let cached = std::fs::read(&bin).unwrap();
    std::fs::remove_file(&bin).unwrap();
    std::fs::remove_dir_all(cache).unwrap();
    std::fs::remove_dir_all(body_cache).unwrap();
    link("85", 0, 0);
    assert_eq!(std::fs::read(&bin).unwrap(), cached);
}
