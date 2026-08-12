//! Shared-ref / `borrow` parameter parsing and type-checking (post-ARC).
//! `borrow` is an ignored synonym of unmarked; there is no use-after-move-on-sink.

mod common;
use common::*;

#[test]
fn unmarked_param_parses_and_typechecks() {
    let code = r#"
        fun shared(s: string): void { }
        fun main(): void {
            let x = "hi";
            shared(x);
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
fn reuse_after_call_is_allowed() {
    let code = format!(
        "{SYSTEM_STUB}
        fun take(s: string): void {{ System.println(s); }}
        fun main(): void {{
            let x = \"hi\";
            take(x);
            System.println(x);
        }}
    "
    );
    let d = analyze_code(&code);
    assert!(!d.has_errors(), "unexpected errors: {:?}", d);
}

#[test]
fn reuse_after_field_store_is_allowed() {
    // Under GC shared-ref ABI, storing a param into a field does not move it.
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
    assert!(!d.has_errors(), "unexpected errors: {:?}", d);
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
