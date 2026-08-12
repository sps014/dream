//! Release codegen of value-type / Span helpers: unmanaged `List<int>.insert` must lower the
//! `Span.copy_from` path to an open-coded `memory.copy` (not an element-assignment loop).

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

/// Body of `(func $Name ...)` up to the next top-level `(func `, or empty if absent.
fn func_body<'a>(wat: &'a str, name: &str) -> &'a str {
    let start_tag = format!("(func ${}", name);
    let Some(start) = wat.find(&start_tag) else {
        return "";
    };
    let after = &wat[start + start_tag.len()..];
    match after.find("\n(func ") {
        Some(rel) => &wat[start..start + start_tag.len() + rel],
        None => &wat[start..],
    }
}

#[test]
fn list_insert_open_codes_span_copy_from() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/list_insert.dream");
    let wat_path = unique_temp_wat("list_insert");
    let src_s = src.to_str().unwrap().to_string();
    let wat_s = wat_path.to_str().unwrap().to_string();

    Compiler::new(Target::Wasm)
        .with_release(true)
        .compile(&src_s, &wat_s)
        .expect("list_insert should compile under --release");

    let wat = fs::read_to_string(&wat_path).expect("wat written");
    let _ = fs::remove_file(&wat_path);

    assert!(
        wat.contains("memory.copy"),
        "unmanaged List.insert path should open-code memory.copy"
    );

    // Prefer full inlining (no leftover helper). When CALLER_BLOCK_CAP blocks folding into a
    // GC-bloated List.insert CFG, the helper itself must still be the thin unmanaged blit —
    // `memory.copy`, no per-element store loop.
    if wat.contains("(call $Span_int_copy_from") || wat.contains("(func $Span_int_copy_from") {
        let body = func_body(&wat, "Span_int_copy_from");
        assert!(
            !body.is_empty(),
            "Span_int_copy_from referenced but not defined:\n{}",
            wat.lines()
                .filter(|l| l.contains("copy_from") || l.contains("Span_int"))
                .take(40)
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            body.contains("memory.copy"),
            "Span_int_copy_from must open-code memory.copy:\n{}",
            body
        );
        assert!(
            !body.contains("(loop "),
            "Span_int_copy_from must not use an element loop:\n{}",
            body
        );
    }
}
