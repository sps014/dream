//! Explicit backend tuning keeps Release ownership semantics at every LLVM/Binaryen level.

use dream::driver::compiler::Compiler;
use dream::driver::wasm_opt::OptLevel;
use dream_mir::backend::Target;
use std::fs;
use std::path::PathBuf;

const ALL_LEVELS: [OptLevel; 7] = [
    OptLevel::O0,
    OptLevel::O1,
    OptLevel::O2,
    OptLevel::O3,
    OptLevel::O4,
    OptLevel::Size,
    OptLevel::SizeAggressive,
];

fn assert_valid_wasm(bytes: &[u8]) {
    wat::parse_bytes(bytes).expect("wasm should parse");
}

#[test]
#[ignore = "Binaryen at every -O level; cargo test --workspace -- --ignored"]
fn release_wasm_preserves_output_at_every_backend_level() {
    let temporary = tempfile::tempdir().unwrap();
    let dream_file = "tests/cases/collection_literals.dream".to_string();
    let expected = fs::read_to_string("tests/cases/collection_literals.expected").unwrap();
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/dream.js");
    for level in ALL_LEVELS {
        let wat = temporary.path().join(format!("{level:?}.wat"));
        Compiler::new(Target::wasm32())
            .with_release(true)
            .with_optimize(Some(level))
            .compile(&dream_file, wat.to_str().unwrap())
            .unwrap_or_else(|e| panic!("Release compile at {level:?}: {e}"));
        let wasm = wat.with_extension("wasm");
        let bytes = fs::read(&wasm).unwrap();
        assert!(!bytes.is_empty(), "empty module at {level:?}");
        assert_valid_wasm(&bytes);
        let runner = wat.with_extension("mjs");
        fs::write(
            &runner,
            crate::e2e_tests::wasm_runner_script(&runtime, &wasm),
        )
        .unwrap();
        let output = std::process::Command::new("node")
            .arg(runner)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{level:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            expected.trim(),
            "{level:?}"
        );
    }
}

#[test]
fn opt_level_parses_expected_strings() {
    use std::str::FromStr;

    assert_eq!(OptLevel::from_str("s").unwrap(), OptLevel::Size);
    assert_eq!(OptLevel::from_str("z").unwrap(), OptLevel::SizeAggressive);
    assert_eq!(OptLevel::from_str("3").unwrap(), OptLevel::O3);
    assert_eq!(OptLevel::from_str("Oz").unwrap(), OptLevel::SizeAggressive);
    assert_eq!(OptLevel::from_str("-Oz").unwrap(), OptLevel::SizeAggressive);
    assert_eq!(OptLevel::from_str("Os").unwrap(), OptLevel::Size);
    assert!(OptLevel::from_str("nope").is_err());
}
