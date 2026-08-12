//! Sink-default / `borrow` parameter parsing, type-checking, and RC store behavior.

mod common;
use common::*;

#[test]
fn sink_param_unmarked_parses_and_typechecks() {
    let code = r#"
        fun sink(s: string): void { }
        fun main(): void {
            let x = "hi";
            sink(x);
        }
    "#;
    let d = analyze_code(code);
    assert!(!d.has_errors(), "unexpected errors: {:?}", d);
}

#[test]
fn borrow_param_is_ok_and_reusable() {
    let code = format!(
        "{SYSTEM_STUB}
        fun peek(borrow s: string): void {{ System.println(s); }}
        fun main(): void {{
            let x = \"hi\";
            peek(x);
            peek(x);
        }}
    "
    );
    let d = analyze_code(&code);
    assert!(!d.has_errors(), "unexpected errors: {:?}", d);
}

#[test]
fn reuse_after_sink_is_allowed_via_copy() {
    // Nim-style: unmarked sink copies when the arg is still live.
    let code = format!(
        "{SYSTEM_STUB}
        fun sink(s: string): void {{ System.println(s); }}
        fun main(): void {{
            let x = \"hi\";
            sink(x);
            System.println(x);
        }}
    "
    );
    let d = analyze_code(&code);
    assert!(!d.has_errors(), "unexpected errors: {:?}", d);
}

#[test]
fn use_after_sink_field_store_is_error() {
    // Storing a sink param into a field moves it — further uses of the param are hard errors
    // (the old silent null caused empty Seq ranges / empty Regex patterns / runaway loops).
    let code = r#"
        class Box {
            public value: string;
            public constructor(value: string) {
                this.value = value;
                let again = value;
            }
        }
        fun main(): void {
            let _ = Box("hi");
        }
    "#;
    let d = analyze_code(code);
    assert!(d.has_errors(), "expected use-after-move error");
    let msg = format!("{:?}", d);
    assert!(
        msg.contains("after move"),
        "unexpected diagnostics: {}",
        msg
    );
}

#[test]
fn borrow_param_reusable_after_field_store() {
    let code = format!(
        "{SYSTEM_STUB}
        class Box {{
            public value: string;
            public constructor(borrow value: string) {{
                this.value = value;
                System.println(value);
            }}
        }}
        fun main(): void {{
            let _ = Box(\"hi\");
        }}
    "
    );
    let d = analyze_code(&code);
    assert!(!d.has_errors(), "unexpected errors: {:?}", d);
}

#[test]
fn sink_store_skips_retain_vs_borrow() {
    // Callee stores a string field: unmarked sink transfers +1 (no retain on store);
    // `borrow` must retain into the field.
    let sink_code = format!(
        "{SYSTEM_STUB}
        class Box {{
            public value: string;
            public constructor(value: string) {{
                this.value = value;
            }}
        }}
        fun main(): void {{
            let s = \"hi\";
            let b = Box(s);
            System.println(b.value);
        }}
    "
    );
    let borrow_code = format!(
        "{SYSTEM_STUB}
        class Box {{
            public value: string;
            public constructor(borrow value: string) {{
                this.value = value;
            }}
        }}
        fun main(): void {{
            let s = \"hi\";
            let b = Box(s);
            System.println(b.value);
        }}
    "
    );
    let sink_wat = emit_hir_to_module_optimized(&sink_code);
    let borrow_wat = emit_hir_to_module_optimized(&borrow_code);
    let sink_retains = sink_wat
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with(";;") && t.contains("call $retain")
        })
        .count();
    let borrow_retains = borrow_wat
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with(";;") && t.contains("call $retain")
        })
        .count();
    assert!(
        sink_retains < borrow_retains,
        "sink constructor should retain less than borrow ({} vs {})\nsink:\n{}\nborrow:\n{}",
        sink_retains,
        borrow_retains,
        sink_wat,
        borrow_wat
    );
}
