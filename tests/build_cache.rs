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
        BuildOutcome::Built(Some(stamp)) => stamp.store(&[out.to_path_buf()]),
        BuildOutcome::Built(None) => panic!("diagnostic-free build should be cacheable"),
        BuildOutcome::Cached(_) => panic!("expected a fresh build"),
    }
}

fn cached(outcome: BuildOutcome) -> Option<Vec<PathBuf>> {
    match outcome {
        BuildOutcome::Cached(artifacts) => Some(artifacts),
        BuildOutcome::Built(_) => None,
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
fn builds_with_warnings_are_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(
        &source,
        "import system;\nfun main(): void { let unused = 1; System.println(\"w\"); }\n",
    )
    .unwrap();
    assert!(matches!(
        build(&source, &out, "O0"),
        BuildOutcome::Built(None)
    ));
}
