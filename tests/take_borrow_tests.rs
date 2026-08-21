//! Sink-default / `borrow` parameter parsing, type-checking, and RC store behavior.

mod common;
use common::*;

/// Extracts the body of the C function `name` (the definition whose signature line ends with '{'),
/// up to its closing brace. Returns "" when no definition is found.
fn c_func_body<'a>(c: &'a str, name: &str) -> &'a str {
    let needle = format!("{name}(");
    let mut from = 0;
    while let Some(i) = c[from..].find(&needle) {
        let hit = from + i;
        let at_word_start = hit == 0 || {
            let b = c.as_bytes()[hit - 1];
            !(b.is_ascii_alphanumeric() || b == b'_')
        };
        let line_start = c[..hit].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = c[hit..].find('\n').map(|p| hit + p).unwrap_or(c.len());
        if at_word_start && c[line_start..line_end].trim_end().ends_with('{') {
            let rest = &c[line_end..];
            return match rest.find("\n}") {
                Some(e) => &rest[..e],
                None => rest,
            };
        }
        from = hit + needle.len();
    }
    ""
}

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
    let sink_c = emit_hir_to_module_optimized(&sink_code);
    let borrow_c = emit_hir_to_module_optimized(&borrow_code);
    // Count `dream_retain(` inside the constructor body only (the module scaffold's
    // object-protocol retain is constant noise otherwise).
    let sink_retains = c_func_body(&sink_c, "Box_constructor")
        .matches("dream_retain(")
        .count();
    let borrow_retains = c_func_body(&borrow_c, "Box_constructor")
        .matches("dream_retain(")
        .count();
    assert!(
        sink_retains < borrow_retains,
        "sink constructor should retain less than borrow ({} vs {})\nsink:\n{}\nborrow:\n{}",
        sink_retains,
        borrow_retains,
        sink_c,
        borrow_c
    );
}

#[test]
fn take_param_array_and_ctor_both_see_value() {
    // `User(name, [name])` must not null `name` when the array is built; both the field and the
    // element should observe the same string.
    let code = format!(
        "{SYSTEM_STUB}
        class Box {{
            public value: string;
            public tags: string[];
            public constructor(value: string, tags: string[]) {{
                this.value = value;
                this.tags = tags;
            }}
        }}
        fun make(name: string): Box {{
            return Box(name, [name]);
        }}
        fun main(): void {{
            let b = make(\"hi\");
            System.println(b.value);
            System.println(b.tags[0]);
        }}
    "
    );
    let c = emit_hir_to_module_optimized(&code);
    assert!(
        c.matches("dream_retain(").count() >= 2,
        "array store of a still-live take param must retain (scaffold has exactly one):\n{}",
        c
    );
    let out = run_and_capture_rc(&code, "main");
    assert_eq!(out.trim(), "hi\nhi");
}
