//! Every integration test, built as one binary so the server library links once.

mod common;

mod followup_tests;
mod lsp_tests;
mod sema_ide_tests;
mod test_crash;
mod test_diag;
mod typed_lsp_tests;

#[test]
fn every_test_file_is_a_module() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let listed = include_str!("integration.rs");
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "rs") && stem != "integration" {
            assert!(
                listed.contains(&format!("\nmod {stem};\n")),
                "tests/{stem}.rs is not built: add `mod {stem};` to tests/integration.rs"
            );
        }
    }
}
