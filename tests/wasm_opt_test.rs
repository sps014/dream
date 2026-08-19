//! Exercises wasm-opt post-processing: compiling with [`Compiler::with_optimize`] must still
//! produce a valid `.wasm` at every level that does not grow vs a release module without Binaryen.

use dream::driver::compiler::{Compiler, Target};
use dream::driver::wasm_opt::OptLevel;
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

fn unique_temp_path(name: &str, ext: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dream_wasm_opt_test_{}_{}.{}",
        name,
        std::process::id(),
        ext
    ));
    path
}

fn assert_valid_wasm(bytes: &[u8]) {
    wat::parse_bytes(bytes).expect("wasm should parse");
}

#[test]
#[ignore = "Binaryen at every -O level; cargo test --workspace -- --ignored"]
fn optimized_wasm_runs_and_is_not_larger_at_every_level() {
    let dream_file = "tests/cases/collection_literals.dream".to_string();

    let plain_wat = unique_temp_path("plain", "wat");
    let plain_wasm = plain_wat.with_extension("wasm");
    let plain_wat_str = plain_wat.to_str().unwrap().to_string();

    // Release trimming without Binaryen — size baseline for each `-O` level below.
    Compiler::new(Target::Wasm)
        .with_release(true)
        .with_optimize(None)
        .compile(&dream_file, &plain_wat_str)
        .expect("unoptimized compile should succeed");
    let plain_bytes = fs::read(&plain_wasm).expect("unoptimized .wasm should exist");
    assert_valid_wasm(&plain_bytes);

    let mut cleanup = vec![
        plain_wat.clone(),
        plain_wasm,
        plain_wat.with_extension("abi.json"),
    ];

    for level in ALL_LEVELS {
        let opt_wat = unique_temp_path(&format!("optimized_{:?}", level), "wat");
        let opt_wasm = opt_wat.with_extension("wasm");
        let opt_wat_str = opt_wat.to_str().unwrap().to_string();

        Compiler::new(Target::Wasm)
            .with_release(true)
            .with_optimize(Some(level))
            .compile(&dream_file, &opt_wat_str)
            .unwrap_or_else(|e| panic!("optimized compile at {:?} should succeed: {:?}", level, e));

        let opt_bytes = fs::read(&opt_wasm)
            .unwrap_or_else(|_| panic!("optimized .wasm at {:?} should exist", level));

        assert!(
            !opt_bytes.is_empty(),
            "optimized .wasm at {:?} is empty",
            level
        );
        assert!(
            opt_bytes.len() <= plain_bytes.len(),
            "wasm-opt at {:?} grew the module: {} -> {} bytes",
            level,
            plain_bytes.len(),
            opt_bytes.len()
        );

        assert_valid_wasm(&opt_bytes);

        cleanup.push(opt_wat.clone());
        cleanup.push(opt_wasm);
        cleanup.push(opt_wat.with_extension("abi.json"));
    }

    for path in cleanup {
        let _ = fs::remove_file(&path);
    }
}

#[test]
fn opt_level_parses_expected_strings() {
    use std::str::FromStr;

    assert_eq!(OptLevel::from_str("s").unwrap(), OptLevel::Size);
    assert_eq!(OptLevel::from_str("z").unwrap(), OptLevel::SizeAggressive);
    assert_eq!(OptLevel::from_str("3").unwrap(), OptLevel::O3);
    assert!(OptLevel::from_str("nope").is_err());
}
