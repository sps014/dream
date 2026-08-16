//! Track A ARC goldens: upper bounds on `$retain` / `$release_` in optimized WAT.

mod common;
use common::*;

fn count_calls(ir: &str, name: &str) -> usize {
    ir.lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with(';') && t.contains(&format!("@{}", name)) && t.contains("call")
        })
        .count()
}

fn llvm_fn_ir<'a>(ir: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("@{}(", name);
    ir.split("\ndefine ").find(|chunk| {
        chunk
            .split('{')
            .next()
            .map(|sig| sig.contains(&needle))
            .unwrap_or(false)
    })
}

fn retain_release_counts(ir: &str) -> (usize, usize) {
    let retains = count_calls(ir, "dream_retain")
        + count_calls(ir, "dream_retain_shared")
        + count_calls(ir, "d_js_retain");
    let releases = count_calls(ir, "dream_release") + count_calls(ir, "d_js_release");
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
    let walk_wat = llvm_fn_ir(&wat, "d_walk").unwrap_or_else(|| llvm_fn_ir(&wat, "d_main").unwrap_or(&wat));
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

/// DOM-shaped `js` temps: last-use destroy must emit `$js_release` in the rebuild function so
/// handles unpin before later work (host `_jsHandles` is not the browser DOM).
#[test]
fn rc_golden_js_rebuild_emits_js_release() {
    let code = r#"
        fun rebuild(): void {
            let container = js.object();
            container.innerHTML = "";
            let row = js.object();
            container.child = row;
            System.println(1 + 2);
        }
        fun main(): void {
            rebuild();
        }
    "#;
    let ir = emit_hir_to_module_optimized(&format!("{}\n{}\n{}", SYSTEM_STUB, JS_STUB, code));
    let rebuild = ir
        .split("define ")
        .find(|s| s.contains("@d_rebuild"))
        .unwrap_or(&ir);
    let js_rel = count_calls(rebuild, "d_js_release");
    assert!(
        js_rel >= 1,
        "rebuild should js_release temps at last use (got {}):\n{}",
        js_rel,
        rebuild
    );
}
