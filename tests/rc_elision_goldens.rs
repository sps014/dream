//! Track A ARC goldens: upper bounds on `$retain` / `$release_` in optimized WAT.

mod common;
use common::*;

fn count_calls(wat: &str, name: &str) -> usize {
    let needle = format!("call ${}", name);
    wat.lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with(";;") && t.contains(&needle)
        })
        .count()
}

fn retain_release_counts(wat: &str) -> (usize, usize) {
    let retains = count_calls(wat, "retain")
        + count_calls(wat, "retain_shared")
        + count_calls(wat, "js_retain");
    let releases = wat
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with(";;") && t.contains("call $release")
        })
        .count();
    (retains, releases)
}

#[test]
fn rc_golden_last_use_return_bounded() {
    let code = r#"
        fun make(): string {
            let s = "hello";
            let t = s;
            return t;
        }
        fun main(): void {
            System.println(make());
        }
    "#;
    let wat = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let (retains, releases) = retain_release_counts(&wat);
    assert!(
        retains <= 4,
        "too many retains ({}) in last-use return:\n{}",
        retains,
        wat
    );
    assert!(
        releases <= 6,
        "too many releases ({}) in last-use return:\n{}",
        releases,
        wat
    );
}

#[test]
fn rc_golden_transparent_if_else_bounded() {
    let code = r#"
        fun pick(c: bool): string {
            let s = "x";
            if (c) {
                return s;
            } else {
                return s;
            }
        }
        fun main(): void {
            System.println(pick(true));
        }
    "#;
    let wat = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let (retains, releases) = retain_release_counts(&wat);
    assert!(
        retains <= 8,
        "too many retains ({}) in if/else:\n{}",
        retains,
        wat
    );
    assert!(
        releases <= 12,
        "too many releases ({}) in if/else:\n{}",
        releases,
        wat
    );
}

#[test]
fn rc_golden_transparent_loop_bounded() {
    let code = r#"
        fun count(n: int): int {
            let s = "keep";
            let i = 0;
            while (i < n) {
                i = i + 1;
            }
            System.println(s);
            return i;
        }
        fun main(): void {
            System.println(count(3));
        }
    "#;
    let wat = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let (retains, releases) = retain_release_counts(&wat);
    assert!(
        retains <= 6,
        "too many retains ({}) around transparent loop:\n{}",
        retains,
        wat
    );
    assert!(
        releases <= 10,
        "too many releases ({}) around transparent loop:\n{}",
        releases,
        wat
    );
}
