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
fn rc_golden_field_walk_near_zero_retains() {
    // Cursor locals: field loads in a loop should not bump RC each iteration.
    let code = r#"
        class Person {
            public name: string;
            public constructor(name: string) {
                this.name = name;
            }
        }
        fun walk(borrow p: Person): int {
            let i = 0;
            let n = 0;
            while (i < 3) {
                n = n + p.name.length;
                i = i + 1;
            }
            return n;
        }
        fun main(): void {
            let p = Person("ada");
            System.println(walk(p));
        }
    "#;
    let wat = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    // Count retains inside the walk function body only (between its func header and the next).
    let walk_wat = wat
        .split("(func $")
        .find(|s| s.starts_with("walk") || s.contains("walk"))
        .unwrap_or(&wat);
    let (retains, _) = retain_release_counts(walk_wat);
    assert!(
        retains <= 2,
        "field-walk should be near retain-free inside walk (got {}):\n{}",
        retains,
        walk_wat
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
