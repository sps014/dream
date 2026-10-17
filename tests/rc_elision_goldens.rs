//! Track A ARC goldens: upper bounds on `dream_retain` / release traffic in the emitted LLVM IR.

use crate::common;
use common::*;

/// A parsed LLVM function definition: its symbol name and body text.
struct IrFunc {
    name: String,
    body: String,
}

/// The symbol name if `line` opens a function definition (`define ... @name(...) ... {`).
fn fn_def_name(line: &str) -> Option<&str> {
    if !line.starts_with("define ") || !line.ends_with('{') {
        return None;
    }
    let after = &line[line.find('@')? + 1..];
    Some(after.split('(').next()?.trim_matches('"'))
}

/// Parses every function definition out of the module; bodies run from after the `define` line
/// to the closing brace at column zero.
fn ir_func_defs(ir: &str) -> Vec<IrFunc> {
    let mut out = Vec::new();
    let mut offset = 0;
    while let Some(line_end) = ir[offset..].find('\n') {
        let line = ir[offset..offset + line_end].trim_end();
        if let Some(name) = fn_def_name(line) {
            let start = offset + line_end + 1;
            let tail = &ir[start..];
            let len = match tail.find("\n}") {
                Some(e) => e,
                None => tail.len(),
            };
            out.push(IrFunc {
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

/// `dream_retain` calls inside user functions only.
fn user_retains(c: &str) -> usize {
    ir_func_defs(c)
        .iter()
        .filter(|f| !is_generated_fn(&f.name))
        .map(|f| count_in(&f.body, "@dream_retain("))
        .sum()
}

/// Release-side traffic (`dream_release` plus per-type `release_*` / `destroy_*` calls) inside
/// user functions only.
fn user_releases(c: &str) -> usize {
    ir_func_defs(c)
        .iter()
        .filter(|f| !is_generated_fn(&f.name))
        .map(|f| {
            count_in(&f.body, "@dream_release(")
                + f.body
                    .lines()
                    .filter(|l| {
                        l.contains("call void @release_") || l.contains("call void @destroy_")
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
    let walk = ir_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "walk")
        .expect("walk should be emitted");
    let retains = count_in(&walk.body, "@dream_retain(");
    assert!(
        retains <= 1,
        "field-walk should be near retain-free inside walk (got {}):\n{}",
        retains,
        c
    );
}

/// The `linked_walk` benchmark shape: a loop-carried `Option` cursor re-seeded from an owned
/// local that stays live across the outer loop. Walking `node.next` must not count anything.
#[test]
fn rc_golden_owner_rooted_list_walk_is_rc_free() {
    let code = r#"
        enum Option<T> { Some(T), None }
        @allow_cycle
        class Node {
            public value: int;
            public next: Option<Node>;
            public constructor(v: int, n: Option<Node>) { this.value = v; this.next = n; }
        }
        @noinline
        fun chain(n: int): Option<Node> {
            let none: Option<Node> = Option.None;
            let head = none;
            let i = 0;
            while i < n { head = Option.Some(Node(i, head)); i = i + 1; }
            return head;
        }
        @noinline
        fun walk_all(iters: int): int {
            let head = chain(64);
            let acc = 0;
            let i = 0;
            while i < iters {
                let curr = head;
                let done = false;
                while done == false {
                    switch (curr) {
                        Some(node) => { acc = acc + node.value; curr = node.next; }
                        None => { done = true; }
                    }
                }
                i = i + 1;
            }
            return acc;
        }
        fun main(): void {
            System.println(walk_all(3));
        }
    "#;
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let walk = ir_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "walk_all")
        .expect("walk_all should be emitted");
    assert_eq!(
        count_in(&walk.body, "@dream_retain("),
        0,
        "list walk retains:\n{}",
        walk.body
    );
    assert_eq!(
        count_in(&walk.body, "@dream_rc_last("),
        1,
        "only the owner's scope exit checks its count:\n{}",
        walk.body
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
    let rebuild = ir_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "rebuild")
        .expect("rebuild should be emitted");
    let js_rel = count_in(&rebuild.body, "@dream_release(");
    assert!(
        js_rel >= 2,
        "rebuild should release its js temps at last use (got {}):\n{}",
        js_rel,
        rebuild.body
    );
}

#[test]
fn rc_golden_discarded_js_call_releases_result() {
    let code = r#"
        fun discard(): void {
            js.global.logger("hello");
        }
        fun main(): void {
            discard();
        }
    "#;
    let c = emit_hir_to_module_optimized(&format!("{}\n{}\n{}", SYSTEM_STUB, JS_STUB, code));
    let discard = ir_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "discard")
        .expect("discard should be emitted");
    let call = discard
        .body
        .find("@dream_js_call(")
        .expect("JS call should remain");
    assert!(
        discard.body[call..].contains("@dream_release("),
        "discarded JS result must be released after the call:\n{}",
        discard.body
    );
}

#[test]
fn rc_golden_local_object_is_scalarized() {
    let code = r#"
        class Box {
            public n: int;
            public tag: string;
            public constructor(n: int) { this.n = n; this.tag = "box"; }
        }
        fun main(): void {
            let b = Box(1);
            let c = b;
            c.tag = c.tag + "!";
            System.println(b.n + c.tag.length);
        }
    "#;
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let main = ir_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "main_dream")
        .expect("main should be emitted");
    for gone in [
        "@dream_malloc(",
        "@dream_frame_object(",
        "@release_Box_into(",
    ] {
        assert_eq!(
            count_in(&main.body, gone),
            0,
            "a Box only read through locals becomes its fields:\n{}",
            main.body
        );
    }
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
            b = Box(2);
            System.println(b.n);
        }
    "#;
    // Two allocations share `b`, so the object stays on the heap rather than being scalarized.
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    assert_eq!(user_retains(&c), 0, "unique Box should not retain:\n{}", c);
    // The last use frees through `Box`'s own destroy tail rather than the generic `dream_release`
    // cascade. It stays guarded by `dream_rc_last`: the intra-procedural Unique lattice proves the
    // *local* holds the only owned reference, not that the object is unaliased, so the unguarded
    // `destroy_Box` form was removed after it caused a use-after-free (a unique local token does not prove object uniqueness).
    let main = ir_func_defs(&c)
        .into_iter()
        .find(|f| f.name == "main_dream")
        .expect("main should be emitted");
    assert!(
        main.body.contains("@release_Box_into("),
        "unique Box should destroy through its own tail:\n{}",
        main.body
    );
    assert!(
        main.body.contains("@dream_rc_last("),
        "the destroy must stay guarded by the last-reference check:\n{}",
        main.body
    );
    assert_eq!(
        count_in(&main.body, "@dream_release("),
        0,
        "unique Box must not fall back to the generic release cascade:\n{}",
        main.body
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

#[test]
fn rc_golden_span_over_borrowed_source_has_no_rc() {
    let code = r#"
        ref struct View {
            src: string;
            offset: int;
            public length: int;
            public constructor(src: string, offset: int, length: int) {
                this.src = src;
                this.offset = offset;
                this.length = length;
            }
            public fun slice(start: int): View {
                return View(this.src, this.offset + start, this.length - start);
            }
        }
        @noinline
        fun view_len(borrow a: string, n: int): int {
            let acc = 0;
            let i = 0;
            while i < n {
                let v = View(a, 0, 4).slice(1);
                acc = acc + v.length;
                i = i + 1;
            }
            return acc;
        }
        fun main(): void {
            System.println(view_len("abcd", 10));
        }
    "#;
    let c = emit_hir_to_module_optimized(&format!("{}\n{}", SYSTEM_STUB, code));
    let body = ir_func_body(&c, "view_len");
    let rc = count_in(body, "@dream_retain(") + count_in(body, "@dream_release(");
    assert_eq!(
        rc, 0,
        "a view over a borrow parameter should not touch RC:\n{body}"
    );
}

#[test]
fn rc_golden_niche_peek_scalar_borrows_but_returned_payload_owns() {
    let code = r#"
        enum Option<T> { Some(T), None }
        class Box {
            public value: string[];
            public constructor(value: string) { this.value = [value]; }
            public fun peek(): Option<string> { return Option.Some(this.value[0]); }
        }
        @export fun peek_length(borrow b: Box): int {
            return switch (b.peek()) { Some(s) => s.length, None => 0 };
        }
        @export fun peek_owned(borrow b: Box): string {
            return switch (b.peek()) { Some(s) => s, None => "" };
        }
    "#;
    let ir = emit_arc_release(code);
    let funcs = ir_func_defs(&ir);
    let scalar = &funcs
        .iter()
        .find(|f| f.name.contains("peek_5flength"))
        .expect("scalar body")
        .body;
    assert_eq!(count_in(scalar, "@dream_retain("), 0, "{scalar}");
    assert_eq!(count_in(scalar, "@dream_release("), 0, "{scalar}");
    let owned = &funcs
        .iter()
        .find(|f| f.name.contains("peek_5fowned"))
        .expect("owned body")
        .body;
    assert!(count_in(owned, "@dream_retain(") > 0, "{owned}");
}

#[test]
fn rc_golden_noninline_string_reader_has_no_rc() {
    let code = r#"
        @noinline fun read_length(s: string): int { return s.length; }
        @export fun read_twice(borrow s: string): int { return read_length(s) + read_length(s); }
    "#;
    let ir = emit_arc_release(code);
    for name in ["read_length", "_Dx10_read_5ftwice"] {
        let body = ir_func_body(&ir, name);
        assert_eq!(count_in(body, "@dream_retain("), 0, "{body}");
        assert_eq!(count_in(body, "@dream_release("), 0, "{body}");
    }
}

#[test]
fn rc_golden_parameter_borrowing_is_independent_of_returned_aliases() {
    let code = r#"
        @noinline fun select(read: string, kept: string): string {
            if read.length == 0 { return kept; }
            return kept;
        }
        @noinline fun length(values: string[]): int { return values[0].length; }
        @export fun select_call(borrow read: string, borrow kept: string): string {
            return select(read, kept);
        }
        @export fun array_call(borrow values: string[]): int { return length(values); }
    "#;
    compile_test_pipeline(code, |hir, interner| {
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        mir.profile = dream_abi::profile::CompileProfile::Release;
        dream_mir::passes::optimize_module_opts(
            &mut mir,
            interner,
            false,
            &mut dream_mir::passes::MirDump::disabled(),
        );
        let select = mir
            .functions
            .iter()
            .find(|f| f.name == "select")
            .expect("select body");
        assert!(!select.locals[select.params[0].0 as usize].is_take);
        assert!(select.locals[select.params[1].0 as usize].is_take);
        let length = mir
            .functions
            .iter()
            .find(|f| f.name == "length")
            .expect("length body");
        assert!(!length.locals[length.params[0].0 as usize].is_take);
    });
    let ir = emit_arc_release(code);
    for name in ["length", "array_call"] {
        let body = ir_func_body(&ir, name);
        assert_eq!(count_in(body, "@dream_retain("), 0, "{body}");
        assert_eq!(count_in(body, "@dream_release("), 0, "{body}");
    }
}

fn emit_arc_release(code: &str) -> String {
    compile_test_pipeline(code, |hir, interner| {
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        mir.profile = dream_abi::profile::CompileProfile::Release;
        dream_mir::passes::optimize_module(&mut mir, interner);
        let pm = dream_mir::passes::PassManager::default_pipeline();
        for f in &mut mir.functions {
            pm.run(f, interner);
        }
        dream_mir::passes::run_late_module_passes(
            &mut mir,
            interner,
            &mut dream_mir::passes::MirDump::disabled(),
        );
        emit_ll(&mir, interner)
    })
}

#[test]
fn rc_golden_flat_record_readers_and_array_projection() {
    let ir = emit_arc_release(
        r#"
        class TextBox {
            public text: string;
            public constructor(text: string) { this.text = text; }
        }
        struct TextRecord {
            public text: string;
            public constructor(text: string) { this.text = text; }
        }
        @noinline fun box_length(value: TextBox): int { return value.text.length; }
        @noinline fun record_length(value: TextRecord): int { return value.text.length; }
        @export fun box_read(borrow value: TextBox): int { return box_length(value); }
        @export fun record_read(borrow value: TextRecord): int { return record_length(value); }
        @export fun projection(borrow values: TextRecord[], index: int): int { return values[index].text.length; }
        @export fun snapshot(borrow values: TextRecord[], index: int): TextRecord { return values[index]; }
    "#,
    );
    for symbol in ["box_length", "record_length", "projection"] {
        let body = ir_func_body(&ir, symbol);
        assert_eq!(count_in(body, "@dream_retain("), 0, "{body}");
        assert_eq!(count_in(body, "@dream_release("), 0, "{body}");
    }
    let body = ir_func_defs(&ir)
        .into_iter()
        .find(|f| f.name.contains("snapshot") && f.body.contains("dream_retain"))
        .expect("owning snapshot implementation")
        .body;
    assert!(
        body.contains("@dream_retain(") || body.contains("retain_"),
        "{body}"
    );
}

#[test]
fn rc_golden_index_scan_borrows_elements_but_overwrites_keep_snapshots() {
    let ir = emit_arc_release(
        r#"
        @export fun index_scan(borrow values: string[]): int {
            let total = 0;
            for (let value in values) {
                total = total + value.length;
            }
            return total;
        }
        @export fun index_overwrite(borrow values: string[], borrow replacement: string): int {
            let value = values[0];
            values[0] = replacement;
            return value.length;
        }
    "#,
    );
    let functions = ir_func_defs(&ir);
    let scan = &functions
        .iter()
        .find(|f| f.name.contains("index_5fscan"))
        .expect("index scan implementation")
        .body;
    assert_eq!(count_in(scan, "@dream_retain("), 0, "{scan}");
    assert_eq!(count_in(scan, "@dream_release("), 0, "{scan}");
    let overwrite = &functions
        .iter()
        .find(|f| f.name.contains("index_5foverwrite"))
        .expect("index overwrite implementation")
        .body;
    assert!(count_in(overwrite, "@dream_retain(") > 0, "{overwrite}");
}

#[test]
fn rc_golden_stdlib_slot_walks_avoid_element_retains() {
    use dream::driver::compiler::Compiler;
    use dream_mir::backend::Target;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("slots.dream");
    let output = dir.path().join("slots.ll");
    std::fs::write(
        &source,
        r#"
        import system;
        @export fun listscan(borrow values: List<string>): int {
            let total = 0;
            for (let value in values) {
                total = total + value.length;
            }
            return total;
        }
        @export fun mapcontains(borrow values: Map<string, string>, borrow key: string): bool {
            return values.contains(key);
        }
        @export fun mutatechildren(borrow values: List<Map<int, int>>): void {
            for (let value in values) {
                value[1] = 2;
            }
        }
        @export fun mapscan(borrow values: Map<string, string>): int {
            let total = 0;
            for (let entry in values) {
                total = total + entry.key.length + entry.value.length;
            }
            return total;
        }
    "#,
    )
    .unwrap();
    Compiler::new(Target::native())
        .with_release(true)
        .compile(&source.display().to_string(), output.to_str().unwrap())
        .unwrap();
    let ir = std::fs::read_to_string(output).unwrap();
    let functions = ir_func_defs(&ir);
    for name in ["listscan", "mapcontains", "mapscan", "mutatechildren"] {
        // Prefer the implementation body over the export trampoline (`entry` → call).
        let f = functions
            .iter()
            .filter(|f| f.name.ends_with(name))
            .max_by_key(|f| f.body.len())
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            count_in(&f.body, "@dream_retain("),
            // Map mutation protects its backing array during stores and rehash. The
            // outer List element needs no additional owning snapshot.
            if name == "mutatechildren" { 1 } else { 0 },
            "{}: {}",
            f.name,
            f.body
        );
        assert!(!f.body.contains("Iterator"), "{}: {}", f.name, f.body);
    }
}
