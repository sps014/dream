//! Release inlining of value-type / Span helpers: `List<int>.insert` must collapse the
//! `Span.copy_from` call layer into an open-coded bulk copy (`memcpy`) on the C→wasm32 path.

use dream::driver::compiler::{Compiler, Target};
use std::fs;
use std::path::PathBuf;

fn unique_temp_wat(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dream_inline_value_{}_{}.wat",
        name,
        std::process::id()
    ));
    path
}

#[test]
fn list_insert_inlines_span_copy_from() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/list_insert.dream");
    let wat_path = unique_temp_wat("list_insert");
    let src_s = src.to_str().unwrap().to_string();
    let wat_s = wat_path.to_str().unwrap().to_string();

    Compiler::new(Target::Wasm32)
        .with_release(true)
        .compile(&src_s, &wat_s)
        .expect("list_insert should compile under --release");

    let wat = fs::read_to_string(&wat_path).expect("wat written");
    let _ = fs::remove_file(&wat_path);

    assert!(
        !wat.contains("copy_from"),
        "Span.copy_from should be inlined away under --release:\n{}",
        wat.lines()
            .filter(|l| l.contains("copy_from") || l.contains("Span"))
            .take(40)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        wat.contains("memory.copy"),
        "unmanaged List.insert path should open-code bulk copies via memory.copy"
    );
}
