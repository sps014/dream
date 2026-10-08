#![cfg(all(feature = "native", unix))]

use std::path::Path;

#[test]
fn debug_reports_an_acknowledged_strong_cycle_instead_of_collecting_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().expect("leak diagnostic directory");
    let source = std::fs::read_to_string(root.join("tests/cases/arc_strong_cycle_leak.dream"))
        .unwrap()
        .replace(
            "System.panic(\"intentional strong-cycle leak\");",
            "System.println(\"intentional strong-cycle leak\");",
        );
    let input = temp.path().join("leak.dream");
    let ir = temp.path().join("leak.ll");
    std::fs::write(&input, source).unwrap();
    dream::driver::compiler::Compiler::new(dream_mir::backend::Target::native())
        .compile(&input.to_string_lossy().into_owned(), &ir.to_string_lossy())
        .unwrap();
    let diagnostic = dream::execution::native::compile_and_capture(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        ir.to_str().unwrap(),
        dream::driver::wasm_opt::OptLevel::O0,
    )
    .expect_err("all-strong cycles must remain allocated")
    .to_string();
    assert!(
        diagnostic.contains("guest leak check live=1"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("[dream] leak check: live=1"),
        "{diagnostic}"
    );
}
