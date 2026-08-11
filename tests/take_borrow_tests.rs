//! `take` / `borrow` parameter parsing and type-checking.

mod common;
use common::*;

#[test]
fn take_param_parses_and_typechecks() {
    let code = r#"
        fun sink(take s: string): void { }
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
fn take_store_skips_retain_vs_borrow() {
    // Callee stores a string field: `take` transfers +1 (no retain on store);
    // unmarked/`borrow` must retain into the field.
    let take_code = format!(
        "{SYSTEM_STUB}
        class Box {{
            public value: string;
            public constructor(take value: string) {{
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
    let take_wat = emit_hir_to_module_optimized(&take_code);
    let borrow_wat = emit_hir_to_module_optimized(&borrow_code);
    let take_retains = take_wat
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
        take_retains < borrow_retains,
        "take constructor should retain less than borrow ({} vs {})\ntake:\n{}\nborrow:\n{}",
        take_retains,
        borrow_retains,
        take_wat,
        borrow_wat
    );
}
