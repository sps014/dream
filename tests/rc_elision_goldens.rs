//! Track A ARC goldens: upper bounds on `dream_retain` / release traffic in optimized C.

mod common;
use common::*;

/// A parsed C function definition: its symbol name and body text.
struct CFunc {
    name: String,
    body: String,
}

const C_KEYWORDS: [&str; 6] = ["if", "else", "while", "for", "switch", "return"];

/// The symbol name if `line` is a function *definition* signature (`... name(params) {`),
/// i.e. not a declaration (`;`), statement, or bare control-flow block.
fn fn_def_name(line: &str) -> Option<&str> {
    if !line.ends_with('{') || line.contains(';') || !line.contains('(') {
        return None;
    }
    let sig = line.trim_end_matches('{').trim();
    let (header, _) = sig.split_once('(')?;
    let mut tokens = header.split_whitespace();
    let name = tokens.next_back()?;
    tokens.next()?; // a return type must precede the name
    if C_KEYWORDS.contains(&name) {
        return None;
    }
    Some(name)
}

/// Parses every function definition out of the emitted C; bodies run from after the signature
/// line to the closing brace at column zero.
fn c_func_defs(c: &str) -> Vec<CFunc> {
    let mut out = Vec::new();
    let mut offset = 0;
    while let Some(line_end) = c[offset..].find('\n') {
        let line = c[offset..offset + line_end].trim_end();
        if let Some(name) = fn_def_name(line) {
            let start = offset + line_end + 1;
            let tail = &c[start..];
            let len = match tail.find("\n}") {
                Some(e) => e,
                None => tail.len(),
            };
            out.push(CFunc {
                name: name.to_string(),
                body: tail[..len].to_string(),
            });
            offset = start + len;
        } else {
            offset += line_end + 1;
        }
    }
    out
}

/// Backend/protocol scaffolding rather than user code (`dream_*` runtime, iface trampolines,
/// generated `to_string`/`hash_code`, deep-release helpers).
fn is_generated_fn(name: &str) -> bool {
    name.starts_with("dream_")
        || name.starts_with("__")
        || name.starts_with("release_")
        || name.starts_with("destroy_")
        || name.ends_with("_to_string")
        || name.ends_with("_hash_code")
        || name == "main"
}

fn count_in(body: &str, needle: &str) -> usize {
    body.matches(needle).count()
}

/// `dream_retain(` calls inside user functions only.
fn user_retains(c: &str) -> usize {
    c_func_defs(c)
        .iter()
        .filter(|f| !is_generated_fn(&f.name))
        .map(|f| count_in(&f.body, "dream_retain("))
        .sum()
}

/// Release-side traffic (`dream_release(` plus per-type `release_*` / `destroy_*` calls) inside
/// user functions only.
fn user_releases(c: &str) -> usize {
    c_func_defs(c)
        .iter()
        .filter(|f| !is_generated_fn(&f.name))
        .map(|f| {
            count_in(&f.body, "dream_release(")
                + f.body
                    .lines()
                    .filter(|l| {
                        let t = l.trim_start();
                        t.starts_with("release_") || t.starts_with("destroy_")
                    })
                    .count()
        })
        .sum()
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
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let retains = user_retains(&c);
    let releases = user_releases(&c);
    assert!(
        retains <= 2,
        "too many retains ({}) in last-use return:\n{}",
        retains,
        c
    );
    assert!(
        releases <= 3,
        "too many releases ({}) in last-use return:\n{}",
        releases,
        c
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
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let retains = user_retains(&c);
    let releases = user_releases(&c);
    assert!(
        retains <= 3,
        "too many retains ({}) in if/else:\n{}",
        retains,
        c
    );
    assert!(
        releases <= 4,
        "too many releases ({}) in if/else:\n{}",
        releases,
        c
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
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let walk = c_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "walk")
        .expect("walk should be emitted");
    let retains = count_in(&walk.body, "dream_retain(");
    assert!(
        retains <= 1,
        "field-walk should be near retain-free inside walk (got {}):\n{}",
        retains,
        c
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
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let retains = user_retains(&c);
    let releases = user_releases(&c);
    assert!(
        retains <= 3,
        "too many retains ({}) around transparent loop:\n{}",
        retains,
        c
    );
    assert!(
        releases <= 4,
        "too many releases ({}) around transparent loop:\n{}",
        releases,
        c
    );
}

/// DOM-shaped `js` temps: last-use destroy must release them in the rebuild function so handles
/// unpin before later work (host `_jsHandles` is not the browser DOM).
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
    let c = emit_hir_to_module_optimized(&format!("{}\n{}\n{}", SYSTEM_STUB, JS_STUB, code));
    let rebuild = c_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "rebuild")
        .expect("rebuild should be emitted");
    let js_rel = count_in(&rebuild.body, "dream_release(");
    assert!(
        js_rel >= 2,
        "rebuild should release its js temps at last use (got {}):\n{}",
        js_rel,
        rebuild.body
    );
}

#[test]
fn rc_golden_unique_class_no_retain() {
    let code = r#"
        class Box {
            public n: int;
            public constructor(n: int) { this.n = n; }
        }
        fun main(): void {
            let b = Box(1);
            System.println(b.n);
        }
    "#;
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    assert_eq!(
        user_retains(&c),
        0,
        "unique Box should not retain:\n{}",
        c
    );
    assert!(
        c.contains("destroy_Box(b);"),
        "unique Box should unique-destroy:\n{}",
        c
    );
}

#[test]
fn rc_golden_last_use_field_store_skips_retain() {
    // A last-use value moved into a field must transfer ownership without a retain.
    let code = r#"
        class Inner {
            public n: int;
            public constructor(n: int) { this.n = n; }
        }
        class Wrap {
            public inner: Inner;
            public constructor(inner: Inner) { this.inner = inner; }
        }
        fun main(): void {
            let w = Wrap(Inner(1));
            System.println(w.inner.n);
        }
    "#;
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    assert_eq!(
        user_retains(&c),
        0,
        "last-use field store should not retain:\n{}",
        c
    );
}
