//! HIR->MIR->LLVM IR emission and native execution tests.
//! Moved out of `dream-sema` so the analyzer crate has no `dream-mir` dependency.

use crate::common;
use common::*;
use dream_diagnostics::DiagnosticBag;
use dream_sema::analyzer::Analyzer;
use dream_syntax::lexer::Lexer;
use dream_syntax::parser::Parser;
use pretty_assertions::assert_eq;

/// Whether `body` does `op` (`"load i32"` / `"store i32"`) through an address derived from a
/// reference load, a field offset (`getelementptr`), or a runtime address call — a real heap
/// access rather than a local slot.
fn heap_access(body: &str, op: &str) -> bool {
    let mut derived = Vec::new();
    for line in body.lines().map(str::trim) {
        let (lhs, rhs) = match line.split_once(" = ") {
            Some((l, r)) => (Some(l), r),
            None => (None, line),
        };
        if let Some(l) = lhs
            && ["load ptr,", "getelementptr ", "call ptr "]
                .iter()
                .any(|p| rhs.starts_with(p))
        {
            derived.push(l);
        }
        if !rhs.starts_with(op) {
            continue;
        }
        let addr = rhs
            .rsplit(", ptr ")
            .next()
            .and_then(|a| a.split(',').next());
        if addr.is_some_and(|a| derived.contains(&a)) {
            return true;
        }
    }
    false
}

/// Whether some block label is branched to from a later block (a loop back-edge).
fn has_back_edge(body: &str) -> bool {
    let mut offset = 0;
    for line in body.lines() {
        offset += line.len() + 1;
        if let Some(label) = line.strip_suffix(':')
            && body[offset.min(body.len())..].contains(&format!("label %{label}"))
        {
            return true;
        }
    }
    false
}

#[test]
fn test_hir_emission_arithmetic_function() {
    // A plain free function over arithmetic on parameters is fully representable in HIR, so the
    // analyzer emits it and it survives the MIR backend pipeline.
    let (c, count) = emit_hir_to_ir("fun add(a: int, b: int): int { return a + b; }");
    assert_eq!(
        count, 1,
        "the single free function should be emitted as HIR"
    );
    assert!(
        c.contains("define internal i32 @add("),
        "missing emitted function:\n{}",
        c
    );
    let body = ir_func_body(&c, "add");
    assert!(
        body.contains(" = add i32 ") && !body.contains("add nsw"),
        "missing wrapping arithmetic:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_locals_and_assignment() {
    // `let` + assignment + return over locals: each statement is supported, so the function emits.
    let code = "fun calc(n: int): int { let x: int = n; let y: int = x + 1; y = y + n; return y; }";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1);
    assert!(
        c.contains("define internal i32 @calc("),
        "missing emitted function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_skips_unsupported_functions() {
    // An uninstantiated generic template (`gen<T>`) has no concrete body to lower until it is
    // monomorphized at a call site, so the interleaved HIR emission skips it. Instantiations are
    // emitted when a call site specializes them. The concrete sibling still emits.
    let code = "
        fun simple(a: int): int { return a; }
        fun gen<T>(x: T): T { return x; }
    ";
    let (_, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 1,
        "only the fully-supported function should be emitted"
    );
}

#[test]
fn test_hir_emission_while_loop() {
    // `while` over locals is now fully representable; the whole function survives the pipeline.
    // Only the comparison shape and the presence of the back-edge are asserted.
    let code = "fun count(n: int): int { let s: int = 0; while (s < n) { s = s + 1; } return s; }";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the while function should be emitted as HIR");
    assert!(
        c.contains("define internal i32 @count("),
        "missing emitted function:\n{}",
        c
    );
    let body = ir_func_body(&c, "count");
    assert!(
        body.contains("icmp slt i32"),
        "missing loop comparison:\n{}",
        c
    );
    assert!(has_back_edge(body), "while should emit a back-edge:\n{}", c);
}

#[test]
fn test_hir_emission_if_else_chain() {
    // `if` / `else if` / `else` folds into nested HIR `If`s and lowers to a branching CFG.
    let code = "
        fun classify(n: int): int {
            if (n < 0) { return 0; } else if (n == 0) { return 1; } else { return 2; }
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 1,
        "the if/else-if/else function should be emitted as HIR"
    );
    assert!(
        c.contains("define internal i32 @classify("),
        "missing emitted function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_for_loop() {
    // A C-style `for (init; cond; step)` desugars to HIR `For` and lowers cleanly.
    let code = "
        fun sum(n: int): int {
            let acc: int = 0;
            for (let i: int = 0; i < n; i = i + 1) { acc = acc + i; }
            return acc;
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the for-loop function should be emitted as HIR");
    assert!(
        c.contains("define internal i32 @sum("),
        "missing emitted function:\n{}",
        c
    );
    let body = ir_func_body(&c, "sum");
    assert!(
        body.contains(" = add i32 ") && !body.contains("add nsw"),
        "missing wrapping arithmetic:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_foreach_loop() {
    // For-each over an array parameter lowers to the indexed-iteration MIR form.
    let code = "
        fun total(xs: int[]): int {
            let acc: int = 0;
            for (let x in xs) { acc = acc + x; }
            return acc;
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the foreach function should be emitted as HIR");
    assert!(
        c.contains("define internal i32 @total("),
        "missing emitted function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_logical_and_ternary() {
    // `&&`/`||` lower to short-circuit control flow; the ternary lowers to a branch + join temp.
    let code = "
        fun pick(a: bool, b: bool, x: int, y: int): int {
            return (a && b) ? x : y;
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 1,
        "the logical/ternary function should be emitted as HIR"
    );
    assert!(
        c.contains("define internal i32 @pick("),
        "missing emitted function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_coalesce() {
    // `lhs ?? rhs` is sugar for `lhs.unwrap_or(rhs)` on an `Option<T>` left operand. This unit test
    // runs outside the stdlib prelude, so it declares a minimal stand-in `Option<T>` with the
    // `unwrap_or` method the desugar dispatches to.
    let code = "
        enum Option<T> { Some(T), None }
        extend Option<T> {
            fun unwrap_or(fallback: T): T {
                return switch (this) { Some(v) => v, None => fallback };
            }
        }
        fun or_default(x: Option<string>): string { return x ?? \"d\"; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 2,
        "unwrap_or and or_default should be emitted as HIR"
    );
    assert!(
        c.contains("or_default("),
        "missing emitted function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_cast() {
    // A numeric widening cast lowers to an int-to-float conversion.
    let code = "fun widen(x: int): double { return (double)x; }";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the cast function should be emitted as HIR");
    let body = ir_func_body(&c, "widen");
    assert!(body.contains("sitofp i32"), "missing widening cast:\n{}", c);
}

#[test]
fn test_hir_emission_index_and_array_literal() {
    // Array literals allocate via `dream_malloc` and store the length + elements; indexing reads
    // through the element address.
    let code = "
        fun first(xs: int[]): int { return xs[0]; }
        fun make(): int[] { return [1, 2, 3]; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 2,
        "both the index and array-literal functions should be emitted"
    );
    assert!(
        c.contains("define internal i32 @first("),
        "missing index function:\n{}",
        c
    );
    assert!(
        c.contains("define internal ptr @make()"),
        "missing array-literal function:\n{}",
        c
    );
    assert!(
        c.contains("dream_malloc("),
        "array literal should allocate:\n{}",
        c
    );
}

#[test]
fn test_empty_array_literal_infers_from_context() {
    // An untyped `[]` resolves its element type from the surrounding context: a `return`, a
    // variable reassignment, a field write, and a call argument. None of these carry an inline
    // `int[]` annotation on the literal itself, so each exercises the expected-type threading.
    let code = "
        fun sink(xs: int[]): int { return 0; }
        class Bag { public items: int[]; public constructor() { this.items = []; } }
        fun make(): int[] { return []; }
        fun driver(): int {
            let ys: int[] = [1];
            ys = [];
            return sink([]);
        }
    ";
    let (c, _count) = emit_hir_to_ir(code);
    assert!(
        c.contains("define internal ptr @make()"),
        "return-context empty array should emit:\n{}",
        c
    );
    assert!(
        c.contains("define internal i32 @driver("),
        "assignment/arg empty array should emit:\n{}",
        c
    );
    assert!(
        c.contains("s0_3_Bag_0_constructor("),
        "field-init empty array should emit:\n{}",
        c
    );
}

#[test]
fn test_nested_empty_array_infers_element_type() {
    // The expected element type is threaded into each element, so the inner `[]` in `int[][] = [[]]`
    // infers `int[]` (rather than being treated as an untyped `int[][]` and mistyping the outer).
    let code = "
        fun driver(): int {
            let g: int[][] = [[]];
            return 0;
        }
    ";
    let diagnostics = analyze_code(code);
    assert!(
        !diagnostics.has_errors(),
        "nested empty array should type-check: {:?}",
        diagnostics.diagnostics
    );
    let (c, _count) = emit_hir_to_ir(code);
    assert!(
        c.contains("define internal i32 @driver("),
        "nested empty array should emit:\n{}",
        c
    );
}

#[test]
fn test_ambiguous_empty_array_reports_clear_error() {
    // Without any array-typed context there is nothing to infer the element type from, so the
    // literal is rejected with an actionable message (and a real span), not silently dropped.
    let code = "
        fun driver(): int {
            let bad = [];
            return 0;
        }
    ";
    let diagnostics = analyze_code(code);
    assert!(
        diagnostics.errors().any(|d| d
            .message
            .contains("infer the element type of an empty array")),
        "expected an actionable empty-array error, got: {:?}",
        diagnostics.diagnostics
    );
}

#[test]
fn test_hir_emission_direct_call() {
    // A direct free-function call resolves to the callee's `DefId` and emits a direct call.
    let code = "
        fun addup(a: int, b: int): int { return a + b; }
        fun driver(): int { return addup(1, 2); }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 2, "both the callee and the caller should be emitted");
    assert!(
        c.contains("define internal i32 @driver("),
        "missing caller:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @addup(i32 1, i32 2)"),
        "call should resolve to the callee symbol:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_extend_nongeneric_class() {
    // An `extend` method is lowered exactly like a struct method (`{Type}_{method}` + `this`), so its
    // body emits and an instance call resolves to it.
    let code = "
        class Point { public x: int; }
        extend Point { public fun getx(): int { return this.x; } }
        fun use_ext(p: Point): int { return p.getx(); }
    ";
    let (c, _count) = emit_hir_to_ir(code);
    assert!(
        c.contains("define internal i32 @s0_5_Point_0_getx("),
        "extend method body should emit:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @s0_5_Point_0_getx(ptr "),
        "call should resolve to the extend method:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_extend_generic_class() {
    // A generic `extend Box<T>` monomorphizes alongside the struct instance: the method is registered
    // under the mangled name (`Box_int_peek`), so its body and call resolve there with no suffix.
    let code = "
        class Box<T> { public v: T; }
        extend Box<T> { public fun peek(): T { return this.v; } }
        fun use_ext(b: Box<int>): int { return b.peek(); }
    ";
    let (c, _count) = emit_hir_to_ir(code);
    assert!(
        c.contains("define internal i32 @s0_3_Box_1_6_p3_5fint_peek("),
        "generic extend method should emit:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @s0_3_Box_1_6_p3_5fint_peek(ptr "),
        "call should resolve to the instance:\n{}",
        c
    );
    assert!(
        !c.contains("Box_int_peek__"),
        "no instance suffix on a struct-generic extend:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_destructor_body() {
    // A `del()` destructor is lowered like any method, so its body emits under `{Type}_del`. (The
    // release-time *invocation* is part of the RC runtime and handled by the release helpers.)
    let code = "
        class Res { public h: int; del() { this.h = 0; } }
        fun mk(): Res { return Res(); }
    ";
    let (c, _count) = emit_hir_to_ir(code);
    assert!(
        c.contains("define internal void @s0_3_Res_0_del("),
        "destructor body should emit:\n{}",
        c
    );
}

#[test]
fn test_release_runtime_deep_release_del_and_dispatch() {
    // The deep-release runtime: each nominal type gets a `release_<Type>` that (when the count hits
    // zero) runs its `del()` destructor, releases reference fields, and frees. `destroy_object`
    // tag-dispatches to those per-type destroys. Non-reference fields (`v: int`) are not released.
    let code = format!(
        "{SYSTEM_STUB}
                class Node {{ public next: Node; public v: int;
            del() {{ System.print(0); }}
            public constructor(v: int) {{ this.v = v; }}
        }}
        fun main(): void {{ let n: Node = Node(1); let o: object = n; }}"
    );
    // RC insertion is required so `main`'s scope-exit releases reference the deep-release runtime;
    // dead-function elimination otherwise (correctly) drops those uncalled helpers. Binding to an
    // `object` local forces a statically-untyped release, exercising the tag-dispatch router.
    let c = emit_hir_to_module_rc_only(&code);
    assert!(
        c.contains("define internal void @release_Node("),
        "per-type release missing:\n{}",
        c
    );
    assert!(
        c.contains("call void @s0_4_Node_0_del("),
        "destructor not invoked from release:\n{}",
        c
    );
    // The reference field `next` is deep-released; the scalar `v` is not.
    assert!(
        c.matches("release_Node(").count() >= 2,
        "reference field not released:\n{}",
        c
    );
    assert!(
        c.contains("define internal void @destroy_object("),
        "tag-dispatch router missing:\n{}",
        c
    );
    assert!(
        c.contains("call void @dream_recycle("),
        "typed last-ref destroy recycles the block:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_user_constructor() {
    // A struct with a user-defined `constructor(...){}`: `Point(1, 2)` allocates, zeroes, and calls the
    // constructor(rather than initializing fields positionally); the constructor body is emitted too.
    let code = "
        class Point {
            public x: int;
            public y: int;
            public constructor(a: int, b: int) { this.x = a; this.y = b; }
        }
        fun make(): Point { return Point(1, 2); }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 2,
        "both the constructor body and make should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("define internal void @s0_5_Point_0_constructor("),
        "constructor body should emit:\n{}",
        c
    );
    assert!(
        c.contains("dream_malloc("),
        "construction should allocate:\n{}",
        c
    );
    assert!(
        c.contains("call void @s0_5_Point_0_constructor(") && c.contains("i32 1, i32 2)"),
        "construction should invoke the user constructor:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_generic_struct_construction_and_field() {
    // Constructing and reading a generic struct instance (`Box<int>`) resolves to the monomorphized
    // layout: `Box<int>(7)` allocates + stores the field, and `b.v` loads it. The per-instance
    // layout is keyed by the interned type, so field widths are correct.
    let code = "
        class Box<T> { public v: T; public constructor(v: T) { this.v = v; } }
        fun make(): Box<int> { return Box<int>(7); }
        fun read(b: Box<int>): int { return b.v; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 3,
        "make, read, and the constructor body should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("dream_malloc("),
        "generic construction should allocate:\n{}",
        c
    );
    assert!(
        c.contains("define internal void @s0_3_Box_1_6_p3_5fint_constructor("),
        "the monomorphized constructor should emit:\n{}",
        c
    );
    let read = ir_func_body(&c, "read");
    assert!(
        heap_access(read, "load i32"),
        "the field read should lower to a load:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_generic_struct_method_instance() {
    // A method on a generic struct is a non-generic method whose specialization is baked into its
    // mangled def name (`Box_int_get`), so its body and call site resolve to that name with no
    // instance suffix — no `def{N}` fallback.
    let code = "
        class Box<T> { public v: T; public fun get(): T { return this.v; } }
        fun use_box(b: Box<int>): int { return b.get(); }
    ";
    let (c, _count) = emit_hir_to_ir(code);
    assert!(
        c.contains("define internal i32 @s0_3_Box_1_6_p3_5fint_get("),
        "generic-struct method body should emit under its mangled name:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @s0_3_Box_1_6_p3_5fint_get(ptr "),
        "instance call should dispatch to the mangled method:\n{}",
        c
    );
    assert!(
        !c.contains("Box_int_get__"),
        "a struct-generic method should NOT carry an instance suffix:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_global_initializer_runs_in_start() {
    // A top-level variable's initializer is captured as the global's `init`; the module synthesizes
    // a `__dream_init` that stores it, and `dream_runtime_init` invokes it before any user code runs.
    let code = "
        let counter: int = 40;
        fun get(): int { return counter; }
    ";
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let parse_arena = bumpalo::Bump::new();
    let mut parser = Parser::new(lexer, &parse_arena, &mut diagnostics);
    let tree = parser.parse().expect("parse should succeed");
    let arena = bumpalo::Bump::new();
    let graph = dream_sema::module_graph::ModuleGraph::single(tree.get_root().clone());
    let mut analyzer = Analyzer::new(&graph, &arena);
    let hir = analyzer
        .analyze(&mut diagnostics)
        .expect("analysis should succeed")
        .hir;
    assert!(!diagnostics.has_errors(), "unexpected analysis errors");

    let interner = analyzer.interner();
    let mir = dream_mir::lower::lower_program(&hir, interner);
    let c = emit_ll(&mir, interner);
    assert!(
        c.contains("define internal void @__dream_init()"),
        "missing init function:\n{}",
        c
    );
    // `__dream_init` is itself invoked from `dream_runtime_init`, which `main` calls before
    // running any user code.
    assert!(
        ir_func_body(&c, "dream_runtime_init").contains("call void @__dream_init()"),
        "init must be invoked from the runtime-init wrapper:\n{}",
        c
    );
    // `g0` is the synthetic `__closure_env` global; `counter` is the first user global, so it lands
    // at `g1`.
    assert!(
        ir_func_body(&c, "__dream_init").contains("store i32 40, ptr @g1"),
        "init should store the global:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_noinline_is_attribute_driven() {
    // Only `@noinline` marks a function noinline; a name containing "sink" gets no special treatment.
    let code = "
        @noinline
        fun keep(x: int): int { return x + 1; }
        fun kitchenSink(x: int): int { return x + 2; }
        fun run(): int { return keep(1) + kitchenSink(2); }
    ";
    let c = emit_hir_to_module(code);
    let define_line = |needle: &str| {
        c.lines()
            .find(|l| l.starts_with("define ") && l.contains(needle))
            .unwrap_or_else(|| panic!("no define for {}:\n{}", needle, c))
            .to_string()
    };
    assert!(define_line("keep").contains("noinline"), "{}", c);
    assert!(!define_line("kitchenSink").contains("noinline"), "{}", c);
}

#[test]
fn test_hir_emission_extern_import_and_call() {
    // An `extern fun` becomes a declaration of its `@js` host symbol, and a call to it resolves to
    // that symbol so the module links.
    let code = "
        @js(\"host\", \"log_it\")
        extern fun log(x: int): void;
        fun run(): void { log(7); }
    ";
    let c = emit_hir_to_module(code);
    assert!(
        c.contains("declare void @log_it(i32)"),
        "extern should declare its @js target:\n{}",
        c
    );
    assert!(
        c.contains("call void @log_it(i32 7)"),
        "call should resolve to the import:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_runtime_import_and_call() {
    let code = "
        @runtime(\"fileRead\")
        extern fun file_read(path: string): string;
        fun run(): string { return file_read(\"x\"); }
    ";
    let c = emit_hir_to_module(code);
    assert!(
        c.contains("fileRead("),
        "runtime extern should bind to its Dream host symbol:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_extern_import_with_result() {
    // A defaulted extern (no `@js`) binds to a plain symbol of the same name.
    let code = "
        extern fun now(): int;
        fun t(): int { return now(); }
    ";
    let c = emit_hir_to_module(code);
    assert!(
        c.contains("declare i32 @now()"),
        "defaulted extern should declare its result-bearing symbol:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @now()"),
        "call should resolve to the symbol:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_print_int_and_println() {
    // `System.print(int)` lowers to `print_int`; `println` adds a trailing newline (`\n` = 10) via
    // `print_char`.
    let code = format!(
        "{SYSTEM_STUB}
        fun run(): void {{
            System.print(41);
            System.println(42);
        }}"
    );
    let c = emit_hir_to_module(&code);
    assert!(
        c.contains("call void @print_int(i32 41)"),
        "print(int) should call print_int:\n{}",
        c
    );
    assert!(
        c.contains("call void @print_char(i32 10)"),
        "println should append a newline via print_char:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_print_string_interns_literal() {
    // `System.print(string)` lowers to `print_string` over the interned literal (a static
    // length-prefixed UTF-16 block: "hi" = {104, 105}).
    let code = format!("{SYSTEM_STUB} fun run(): void {{ System.print(\"hi\"); }}");
    let c = emit_hir_to_module(&code);
    assert!(
        c.contains("call void @print_string(ptr getelementptr (i8, ptr @__ds"),
        "print(string) should call print_string:\n{}",
        c
    );
    assert!(
        c.contains("[i16 104, i16 105]"),
        "the string literal should be interned:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_print_char() {
    let code = format!("{SYSTEM_STUB} fun run(): void {{ System.print('x'); }}");
    let c = emit_hir_to_module(&code);
    assert!(
        c.contains("print_char("),
        "print(char) should call print_char:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_print_bool_float_double_long() {
    // Non-`int`/`char`/`string` scalars render through their `*_to_string` formatter (float/double
    // go through the host `print_float`/`print_double`) then print as strings.
    let code = format!(
        "{SYSTEM_STUB}
        fun run(b: bool, f: float, d: double, l: long): void {{
            System.print(b);
            System.print(f);
            System.print(d);
            System.print(l);
        }}"
    );
    let c = emit_hir_to_module(&code);
    for helper in [
        "dream_bool_to_string(",
        "print_float(",
        "print_double(",
        "dream_long_to_string(",
    ] {
        assert!(c.contains(helper), "missing {helper} in print:\n{}", c);
    }
}

#[test]
fn test_hir_emission_print_object_routes_to_print_object() {
    // Printing an object renders through the generated default `{Type}_to_string`, then prints the
    // resulting string (the WAT backend's tag-dispatching `$print_object` call site is gone; the
    // router survives as `dream_print_object` for the dynamic-`object` path).
    let code = format!(
        "{SYSTEM_STUB}
        class Box {{ public v: int; }}
        fun run(b: Box): void {{ System.print(b); }}"
    );
    let c = emit_hir_to_module(&code);
    assert!(
        c.contains("define internal void @run("),
        "an object print should be covered now:\n{}",
        c
    );
    assert!(
        c.contains("call ptr @Box_to_string(ptr "),
        "object print routes through the generated to_string:\n{}",
        c
    );
    assert!(
        c.contains("define internal ptr @Box_to_string(ptr %a0)"),
        "a default struct to_string is generated:\n{}",
        c
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_print_int_and_arithmetic() {
    // Runs a real program through the MIR backend: `print` of an int literal and of a computed sum,
    // proving the host import + integer path execute end-to-end.
    let code = format!(
        "{SYSTEM_STUB}
        fun main(): void {{
            System.print(41);
            System.print(1 + 1);
        }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "412");
}

#[cfg(feature = "native")]
#[test]
fn exec_println_int_appends_newline() {
    let code = format!("{SYSTEM_STUB} fun main(): void {{ System.println(7); }}");
    assert_eq!(run_and_capture(&code, "main"), "7\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_int_to_string_via_concat_and_interpolation() {
    // A non-string operand of `+` (and any interpolation hole) is implicitly rendered through the
    // object protocol's `to_string`, so `int` values compose into strings with no explicit call.
    let code = format!(
        "{SYSTEM_STUB}
        fun main(): void {{
            let n: int = 42;
            System.println(\"count = \" + n);
            System.println($\"n is {{n}} and n+1 is {{n + 1}}\");
        }}"
    );
    assert_eq!(
        run_and_capture(&code, "main"),
        "count = 42\nn is 42 and n+1 is 43\n"
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_print_string_literal() {
    // Validates the reconciled string ABI: the interned literal's data pointer is a length-prefixed
    // heap string the host reads correctly.
    let code = format!("{SYSTEM_STUB} fun main(): void {{ System.println(\"hello\"); }}");
    assert_eq!(run_and_capture(&code, "main"), "hello\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_print_bool_via_to_string() {
    // Exercises the bundled `*_to_string` runtime: `bool` renders through `dream_bool_to_string`,
    // whose interned "true"/"false" are printed as length-prefixed strings.
    let code = format!(
        "{SYSTEM_STUB}
        fun main(): void {{
            System.println(true);
            System.println(false);
        }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "true\nfalse\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_string_len_via_strlen() {
    // `str.length` calls `dream_str_len` (UTF-16 code-unit count).
    let code = format!(
        "{SYSTEM_STUB}
        fun main(): void {{
            let s: string = \"hello\";
            System.print(s.length);
        }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "5");
}

#[cfg(feature = "native")]
#[test]
fn exec_print_long_literal_via_to_string() {
    // A magnitude-typed `long` literal stays 64-bit through lowering and renders via
    // `dream_long_to_string`.
    let code = format!("{SYSTEM_STUB} fun main(): void {{ System.println(123456789012); }}");
    assert_eq!(run_and_capture(&code, "main"), "123456789012\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_long_arithmetic_stays_i64() {
    // Exercises the i64 add path end-to-end: two `long` locals summed and printed.
    let code = format!(
        "{SYSTEM_STUB}
        fun main(): void {{
            let a: long = 100000000000;
            let b: long = 23456789012;
            System.println(a + b);
        }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "123456789012\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_print_struct_via_object_to_string() {
    // Object print end-to-end: `Point(1, 2)` allocates a tagged struct, and printing routes through
    // the generated `Point_to_string` to render `Point { x: 1, y: 2 }`.
    let code = format!(
        "{SYSTEM_STUB}
        class Point {{ public x: int; public y: int; public constructor(x: int, y: int) {{ this.x = x; this.y = y; }} }}
        fun main(): void {{ System.println(Point(1, 2)); }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "Point { x: 1, y: 2 }\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_print_nested_struct() {
    // A struct field that is itself a struct renders recursively via the object protocol.
    let code = format!(
        "{SYSTEM_STUB}
        class Point {{ public x: int; public y: int; public constructor(x: int, y: int) {{ this.x = x; this.y = y; }} }}
        class Line {{ public a: Point; public b: Point; public constructor(a: Point, b: Point) {{ this.a = a; this.b = b; }} }}
        fun main(): void {{ System.println(Line(Point(1, 2), Point(3, 4))); }}"
    );
    assert_eq!(
        run_and_capture(&code, "main"),
        "Line { a: Point { x: 1, y: 2 }, b: Point { x: 3, y: 4 } }\n"
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_print_union_variants() {
    // Union print: the tag-dispatched `{Union}_to_string` reads the discriminant and renders the
    // active variant. Data variants render `Variant(field: value, ...)`; unit variants render bare.
    let code = format!(
        "{SYSTEM_STUB}
        enum Shape {{ Circle(int), Rect(width: int, height: int), Empty }}
        fun main(): void {{
            System.println(Shape.Circle(5));
            System.println(Shape.Rect(2, 3));
            System.println(Shape.Empty);
        }}"
    );
    assert_eq!(
        run_and_capture(&code, "main"),
        "Circle(5)\nRect(width: 2, height: 3)\nEmpty\n"
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_print_int_array() {
    // Array print: the element-typed array renderer produces `[e0, e1, ...]`.
    let code = format!(
        "{SYSTEM_STUB} fun main(): void {{ let xs: int[] = [10, 20, 30]; System.println(xs); }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "[10, 20, 30]\n");
}

#[cfg(feature = "native")]
#[test]
fn exec_print_struct_array() {
    // An array of structs renders each element via the struct's `to_string`.
    let code = format!(
        "{SYSTEM_STUB}
        class Point {{ public x: int; public y: int; public constructor(x: int, y: int) {{ this.x = x; this.y = y; }} }}
        fun main(): void {{
            let ps: Point[] = [Point(1, 2), Point(3, 4)];
            System.println(ps);
        }}"
    );
    assert_eq!(
        run_and_capture(&code, "main"),
        "[Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]\n"
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_del_runs_at_last_release() {
    // Overwriting a reference local releases its previous occupant; at refcount zero the deep-release
    // runtime runs the object's `del()` (prints 9 here) before freeing. So `Res(1)` is released (9)
    // when `r` is reassigned, the surviving `Res(2)` prints its field (2), and finally the scope-exit
    // release of `r` runs `Res(2).del()` (9) at function return -> "929". Proves overwrite release,
    // `release_Res` -> `Res_del` -> `dream_free`, and scope-exit release all fire end-to-end.
    let code = format!(
        "{SYSTEM_STUB}
        class Res {{ public v: int;
            del() {{ System.print(9); }}
            public constructor(v: int) {{ this.v = v; }}
        }}
        fun main(): void {{
            let r: Res = Res(1);
            r = Res(2);
            System.print(r.v);
        }}"
    );
    assert_eq!(run_and_capture_rc(&code, "main"), "929");
}

#[test]
fn exec_container_store_retains_no_double_free() {
    // Storing a borrowed reference into a container field retains it, so the field and the source
    // local each own a count. At scope exit both `a` and `b` are released: releasing `a` runs its
    // `del()` (1) and deep-releases `a.next` (dropping `b` to 1), then releasing `b` runs its `del()`
    // (1) and frees it. Each object is destroyed exactly once -> "011". Without the container retain
    // this double-frees `b`.
    let code = format!(
        "{SYSTEM_STUB}
                class Node {{ public next: Node;
            del() {{ System.print(1); }}
            public constructor() {{ }}
        }}
        fun main(): void {{
            let a: Node = Node();
            let b: Node = Node();
            a.next = b;
            System.print(0);
            let keep = a;
        }}"
    );
    assert_eq!(run_and_capture_rc(&code, "main"), "011");
}

#[test]
fn exec_returned_value_transfers_ownership() {
    // `make()` returns an owned local; its `+1` transfers to the caller instead of being released at
    // `make`'s scope exit (which would run `del()` early and hand back a dangling pointer). So `y.v`
    // reads 5, and the object's single `del()` (7) fires only at `main`'s scope exit -> "57".
    let code = format!(
        "{SYSTEM_STUB}
        class R {{ public v: int;
            del() {{ System.print(7); }}
            public constructor(v: int) {{ this.v = v; }}
        }}
        fun make(): R {{
            let x: R = R(5);
            return x;
        }}
        fun main(): void {{
            let y: R = make();
            System.print(y.v);
        }}"
    );
    assert_eq!(run_and_capture_rc(&code, "main"), "57");
}

/// Hand-builds a two-function MIR that takes `add` as a first-class value and calls it indirectly:
/// `fun main() { let f = add; print(f(2, 3)); }`. The analyzer now emits function values itself (see
/// `test_hir_emission_first_class_function`); this hand-built MIR still exercises the backend
/// (FuncRef -> function-table index, function table registration, indirect call through `dream_ft`)
/// in isolation. Returns the interner alongside so its `TypeId`s stay valid.
fn indirect_call_demo() -> (dream_mir::Mir, dream_types::TypeInterner) {
    use dream_mir::build::FunctionBuilder;
    use dream_mir::{BinOp, Callee, Const, Mir, Operand, Place, Rvalue, Statement, Terminator};
    use dream_types::{DefId, TypeInterner};

    let mut i = TypeInterner::new();
    let int = i.int();
    let void = i.void();
    let functy = i.func(vec![int, int], int);
    let add_def = DefId::root(10);

    let mut ab = FunctionBuilder::new("add", int);
    ab.set_def(add_def, vec![]);
    let a = ab.new_param(int, Some("a".into()));
    let b = ab.new_param(int, Some("b".into()));
    let t = ab.new_temp(int);
    ab.assign(
        Place::Local(t),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(a)),
            Operand::Copy(Place::Local(b)),
        ),
    );
    ab.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mut mb = FunctionBuilder::new("main", void);
    mb.set_def(DefId::root(11), vec![]);
    let f = mb.new_local(int, Some("f".into()));
    let r = mb.new_local(int, Some("r".into()));
    mb.assign(
        Place::Local(f),
        Rvalue::FuncRef(Callee {
            def: add_def,
            args: vec![],
            ret: int,
            take_params: vec![],
        }),
    );
    mb.assign(
        Place::Local(r),
        Rvalue::IndirectCall {
            target: Operand::Copy(Place::Local(f)),
            sig: functy,
            args: vec![Operand::Const(Const::Int(2)), Operand::Const(Const::Int(3))],
        },
    );
    mb.push(Statement::Print {
        arg: Operand::Copy(Place::Local(r)),
        ty: int,
        newline: false,
    });
    mb.terminate(Terminator::Return(None));

    (
        Mir {
            functions: vec![ab.finish(), mb.finish()],
            ..Default::default()
        },
        i,
    )
}

#[test]
fn test_indirect_call_emits_table_and_signature() {
    let (mir, interner) = indirect_call_demo();
    let c = emit_ll(&mir, &interner);
    assert!(
        c.contains("@dream_ft = internal constant ["),
        "function table missing:\n{}",
        c
    );
    assert!(
        c.contains("ptr @add"),
        "callee must be registered in the table:\n{}",
        c
    );
    assert!(
        c.contains(" = call i32 %"),
        "indirect-call pointer signature missing:\n{}",
        c
    );
    assert!(
        c.contains("ptr @dream_ft, i64"),
        "indirect call through the table missing:\n{}",
        c
    );
    assert!(c.contains("dream_ft_get"), "table accessor missing:\n{}", c);
}

#[cfg(feature = "native")]
#[test]
fn exec_indirect_call_through_function_table() {
    let code = format!(
        "{SYSTEM_STUB}
        {CLOSURE_STUB}
        fun add(a: int, b: int): int {{ return a + b; }}
        fun main(): void {{ let f = add; System.print(f(2, 3)); }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "5");
}

#[test]
fn test_hir_emission_first_class_function() {
    // A bare function name is a value (`Binding::Func`), and calling a function-typed local emits an
    // `IndirectCall` — both are now HIR-representable, so `main` stays in coverage. The value is
    // boxed as a funcbox and invoked through the function table.
    let code = format!(
        "{SYSTEM_STUB}
        {CLOSURE_STUB}
        fun add(a: int, b: int): int {{ return a + b; }}
        fun main(): void {{ let f = add; System.print(f(2, 3)); }}"
    );
    let c = emit_hir_to_module(&code);
    assert!(
        c.contains("dream_funcbox_new("),
        "function value not boxed:\n{}",
        c
    );
    let main_body = ir_func_body(&c, "main_dream");
    assert!(
        main_body.contains("ptr @dream_ft, i64"),
        "indirect call through the function table not emitted:\n{}",
        c
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_first_class_function_from_source() {
    // Full pipeline: source with a first-class function -> analyzer HIR -> MIR -> table dispatch.
    let code = format!(
        "{SYSTEM_STUB}
        {CLOSURE_STUB}
        fun add(a: int, b: int): int {{ return a + b; }}
        fun main(): void {{ let f = add; System.print(f(2, 3)); }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "5");
}

#[cfg(feature = "native")]
#[test]
fn exec_print_function_value() {
    // Printing a `fun(...)` value renders its static type spelling (funcboxes are untagged).
    let code = format!(
        "{SYSTEM_STUB}
        {CLOSURE_STUB}
        fun add(a: int, b: int): int {{ return a + b; }}
        fun main(): void {{ let f: fun(int, int): int = add; System.println(f); }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "fun(int, int): int\n");
}

#[test]
fn print_function_value_emits_hir() {
    let code = format!(
        "{SYSTEM_STUB}
        {CLOSURE_STUB}
        fun add(a: int, b: int): int {{ return a + b; }}
        fun main(): void {{ let f: fun(int, int): int = add; System.println(f); }}"
    );
    let diagnostics = analyze_code(&code);
    assert!(
        !diagnostics.has_errors(),
        "printing a function value should be supported, got: {:?}",
        diagnostics.diagnostics
    );
}

#[test]
fn func_value_argument_is_reference_counted() {
    // A `fun(...)` value is a heap funcbox (`TyKind::Func` is a reference), so the RC pass retains
    // and releases it like any other managed ref. A `string` bound alongside it is also counted —
    // both should see Retain/Release traffic after RC insertion.
    let code = format!(
        "{SYSTEM_STUB}
        {CLOSURE_STUB}
        fun twice(x: int): int {{ return x * 2; }}
        fun apply(f: fun(int): int, s: string): int {{ return f(3); }}
        fun main(): void {{
            let g: fun(int): int = twice;
            let s: string = \"hi\";
            let r: int = apply(g, s);
        }}"
    );

    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let parse_arena = bumpalo::Bump::new();
    let mut parser = Parser::new(lexer, &parse_arena, &mut diagnostics);
    let tree = parser.parse().expect("parse should succeed");
    let arena = bumpalo::Bump::new();
    let graph = dream_sema::module_graph::ModuleGraph::single(tree.get_root().clone());
    let mut analyzer = Analyzer::new(&graph, &arena);
    let hir = analyzer
        .analyze(&mut diagnostics)
        .expect("analysis should succeed")
        .hir;
    assert!(!diagnostics.has_errors(), "unexpected analysis errors");
    let interner = analyzer.interner();
    let mut mir = dream_mir::lower::lower_program(&hir, interner);
    use dream_mir::passes::MirPass;
    for f in &mut mir.functions {
        dream_mir::passes::RcInsertion.run(f, interner);
    }

    use dream_mir::{Operand, Place, Statement};
    let main = mir
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("main should be lowered");

    let mut func_value_moved = false;
    let mut func_value_rc = 0usize;
    let mut reference_rc = 0usize;
    for block in &main.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Retain(o) | Statement::Release(o) => {
                    if let Operand::Copy(Place::Local(l)) = o {
                        let ty = main.locals[l.0 as usize].ty;
                        if matches!(interner.kind(ty), dream_types::TyKind::Func(_, _)) {
                            func_value_rc += 1;
                        } else if interner.is_reference(ty) {
                            reference_rc += 1;
                        }
                    }
                }
                Statement::Assign(
                    Place::Local(l),
                    dream_mir::Rvalue::Use(Operand::Const(dream_mir::Const::Null)),
                ) => {
                    let ty = main.locals[l.0 as usize].ty;
                    if matches!(interner.kind(ty), dream_types::TyKind::Func(_, _)) {
                        func_value_moved = true;
                    }
                }
                _ => {}
            }
        }
    }

    assert!(
        func_value_rc > 0 || func_value_moved,
        "a function value is a heap funcbox: last-use moves into a sink or is retain/released:\n{:#?}",
        main
    );
    assert!(
        reference_rc > 0,
        "the string local should still be reference-counted:\n{:#?}",
        main
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_print_escapes_in_string_literal() {
    // The literal-unescaping in HIR emission turns `\t` into a real tab and drops the source quotes.
    let code = format!("{SYSTEM_STUB} fun main(): void {{ System.print(\"a\\tb\"); }}");
    assert_eq!(run_and_capture(&code, "main"), "a\tb");
}

#[test]
fn test_hir_emission_generic_function_instances() {
    // A generic free function is emitted once per monomorphization: `id(5)` and `id(true)` produce
    // two instance bodies with distinct symbols, and each call site resolves to its instance.
    let code = "
        fun id<T>(x: T): T { return x; }
        fun driver(): int { let a: int = id(5); let b: bool = id(true); return a; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 3,
        "two id instances + driver should be emitted:\n{}",
        c
    );
    let types = dream_types::TypeInterner::new();
    let defs = dream_types::DefTable::new();
    let int_symbol = dream_types::function_symbol(
        None,
        "id",
        &[dream_types::type_symbol(&types, &defs, types.int())],
    );
    let bool_symbol = dream_types::function_symbol(
        None,
        "id",
        &[dream_types::type_symbol(&types, &defs, types.bool())],
    );
    assert!(
        c.contains(&format!("{int_symbol}(")) && c.contains(&format!("{bool_symbol}(")),
        "each monomorphization gets its own symbol:\n{}",
        c
    );
    assert!(
        c.contains(&format!("call i32 @{int_symbol}(i32 5)"))
            && c.contains(&format!("call i32 @{bool_symbol}(i32 1)")),
        "each generic call site should resolve to an instance symbol:\n{}",
        c
    );
}

#[test]
fn function_symbols_survive_unrelated_declarations() {
    let source = r#"
        class User {}
        fun id<T>(value: T): T { return value; }
        fun pick(value: User): int { return 1; }
        fun pick(value: int): int { return 2; }
        fun driver(value: User): User { return id<User>(value); }
    "#;
    let symbols = |code: &str| {
        compile_test_pipeline(code, |hir, _| {
            hir.functions
                .iter()
                .filter(|f| f.name == "id" || f.name.starts_with("pick."))
                .map(|f| (f.name.clone(), f.symbol.clone()))
                .collect::<Vec<_>>()
        })
    };
    let before = symbols(source);
    let after = symbols(&format!("class Unrelated {{}}\n{source}"));
    assert_eq!(before.len(), 3);
    assert_eq!(before, after);
}

#[test]
fn test_hir_emission_string_literal() {
    // A string literal resolves to its interned static data block (`__ds<N>`), laid out after the
    // runtime's own constants.
    let code = "fun greet(): string { return \"hi\"; }";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 1,
        "the string-returning function should be emitted as HIR"
    );
    assert!(
        c.contains("define internal ptr @greet()"),
        "missing emitted function:\n{}",
        c
    );
    let greet = ir_func_body(&c, "greet");
    assert!(
        greet.contains("__ds"),
        "string literal should resolve to an interned data pointer:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_field_read_and_constructor() {
    // A struct-field read and a (non-generic) constructor are both representable; field indexing is
    // resolved from the struct layout and `new` resolves the struct's `DefId`.
    let code = "
        class Point { public x: int; public y: int; public constructor(x: int, y: int) { this.x = x; this.y = y; } }
        fun getx(p: Point): int { return p.x; }
        fun make(): Point { return Point(1, 2); }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 3,
        "the field-read, constructor, and constructor-body functions should be emitted"
    );
    assert!(
        c.contains("define internal i32 @getx("),
        "missing field-read function:\n{}",
        c
    );
    assert!(
        c.contains("define internal ptr @make()"),
        "missing constructor function:\n{}",
        c
    );
    // `p.x` (field 0) lowers to a real load now that the layout is threaded through.
    let getx = ir_func_body(&c, "getx");
    assert!(
        heap_access(getx, "load i32"),
        "field read should lower to a load:\n{}",
        c
    );
    // `Point(1, 2)` allocates and initializes fields.
    assert!(
        c.contains("dream_malloc("),
        "constructor should allocate:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_field_assignment() {
    // Writing through a struct field lowers to an `Assign` with a `Field` place.
    let code = "
        class Counter { public n: int; }
        fun bump(c: Counter): void { c.n = c.n + 1; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the field-assignment function should be emitted");
    assert!(
        c.contains("define internal void @bump("),
        "missing field-assignment function:\n{}",
        c
    );
    // `c.n = ...` lowers to a real store through the field address.
    let bump = ir_func_body(&c, "bump");
    assert!(
        heap_access(bump, "store i32"),
        "field write should lower to a store:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_index_assignment() {
    // Indexed assignment lowers to an `Assign` with an `Index` place.
    let code = "fun setfirst(xs: int[], v: int): void { xs[0] = v; }";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the index-assignment function should be emitted");
    assert!(
        c.contains("define internal void @setfirst("),
        "missing index-assignment function:\n{}",
        c
    );
    // `xs[0] = v` computes the element address (base + 4 + i*stride) and stores.
    let setfirst = ir_func_body(&c, "setfirst");
    assert!(
        heap_access(setfirst, "store i32"),
        "index write should lower to a store:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_enum_value() {
    // An enum-member reference resolves to its constant integer value.
    let code = "
        enum Color { Red, Green, Blue }
        fun pick(): Color { return Color.Green; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the enum-returning function should be emitted");
    assert!(
        c.contains("define internal i32 @pick("),
        "missing enum function:\n{}",
        c
    );
    // `Color.Green` is the second member, value 1.
    let pick = ir_func_body(&c, "pick");
    assert!(pick.contains("ret i32 1"), "missing enum constant:\n{}", c);
}

#[test]
fn test_hir_emission_method_body_and_instance_call() {
    // A method body (with a `this` receiver and a field read) is emitted under its mangled name,
    // and a resolved instance-method call lowers to a direct call.
    let code = "
        class Box { public v: int; public fun get(): int { return this.v; } }
        fun use_box(b: Box): int { return b.get(); }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 2,
        "both the method body and its caller should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("define internal i32 @s0_3_Box_0_get("),
        "missing emitted method body:\n{}",
        c
    );
    assert!(
        c.contains("define internal i32 @use_box("),
        "missing instance-call function:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @s0_3_Box_0_get(ptr "),
        "instance call should dispatch to the method:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_static_call() {
    // A (non-generic) static method is a free function under its mangled `{Type}_{method}` name;
    // calling it lowers to a direct call.
    let code = "
        class M { public static fun id(n: int): int { return n; } }
        fun use_static(): int { return M.id(7); }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 2,
        "both the static method and its caller should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("define internal i32 @s0_1_M_0_id("),
        "missing emitted static method:\n{}",
        c
    );
    assert!(
        c.contains("call i32 @s0_1_M_0_id(i32 7)"),
        "static call should dispatch to the method:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_global_read_and_write() {
    // A module-global resolves to a module-scope global for both reads and assignments.
    let code = "
        let counter: int = 0;
        fun tick(): int { counter = counter + 1; return counter; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 1,
        "the global-using function should be emitted:\n{}",
        c
    );
    // `g0` is the synthetic `__closure_env` global; `counter` is the first user global, so it lands
    // at `g1`.
    let tick = ir_func_body(&c, "tick");
    assert!(
        tick.contains("load i32, ptr @g1")
            && tick.contains(" = add i32 ")
            && tick
                .lines()
                .any(|l| l.trim().starts_with("store i32 %") && l.contains("ptr @g1")),
        "missing global read+write:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_union_construction() {
    // Constructing a (non-generic) discriminated-union variant lowers to a `UnionNew`. `Shape` has
    // only primitive payloads, so it is inferred as a *value union*: built into a stack scratch
    // buffer whose first word is the variant discriminant (boxed into a tagged heap block only when
    // returned, unlike the WAT backend which kept it unboxed).
    let code = "
        enum Shape { Circle(int), Empty }
        fun mk(): Shape { return Shape.Circle(2); }
        fun nil(): Shape { return Shape.Empty; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 2,
        "both union constructors should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("define internal void @mk(ptr "),
        "missing data-variant constructor:\n{}",
        c
    );
    assert!(
        c.contains("define internal void @nil(ptr "),
        "missing unit-variant constructor:\n{}",
        c
    );
    // The first word of the union block is the variant discriminant.
    let mk = ir_func_body(&c, "mk");
    let nil = ir_func_body(&c, "nil");
    assert!(
        mk.contains("store i32 0, ptr %"),
        "data variant should store discriminant 0:\n{}",
        c
    );
    assert!(
        nil.contains("store i32 1, ptr %"),
        "unit variant should store discriminant 1:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_switch_statement() {
    // A `switch` with single-label cases and a `default` lowers to `HStmt::Switch`.
    let code = "
        fun classify(n: int): int {
            let r: int = 0;
            switch (n) {
                case 1: r = 10;
                case 2: r = 20;
                default: r = 30;
            }
            return r;
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the switch function should be emitted:\n{}", c);
    assert!(
        c.contains("define internal i32 @classify("),
        "missing switch function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_switch_statement_with_variant_binding() {
    // A statement-position pattern `switch` lowers to `HStmt::Switch`; a variant pattern binds its
    // payload to fresh locals that the arm body resolves.
    let code = "
        enum Shape { Circle(int), Empty }
        fun describe(s: Shape): int {
            let r: int = 0;
            switch (s) {
                Circle(rad) => { r = rad; }
                Empty => { r = 0; }
            }
            return r;
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 1, "the switch function should be emitted:\n{}", c);
    assert!(
        c.contains("define internal i32 @describe("),
        "missing switch function:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_len_builtin() {
    // `arr.length` reads the array's stored length word; `str.length` calls `dream_str_len`
    // (UTF-16 unit count) — both O(1).
    let code = "
        fun count(xs: int[]): int { return xs.length; }
        fun slen(s: string): int { return s.length; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 2, "both size functions should be emitted:\n{}", c);
    assert!(
        c.contains("define internal i32 @count("),
        "missing array-len function:\n{}",
        c
    );
    assert!(
        c.contains("define internal i32 @slen("),
        "missing string-len function:\n{}",
        c
    );
    let slen = ir_func_body(&c, "slen");
    assert!(
        slen.contains("dream_str_len("),
        "string len should call dream_str_len:\n{}",
        c
    );
    let count_body = ir_func_body(&c, "count");
    assert!(
        heap_access(count_body, "load i32") && !count_body.contains("call "),
        "array len should be an inlined length-word load:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_switch_expression() {
    // A value-position `switch` desugars to a result temp + `Switch`, read back as the switch value.
    let code = "
        enum Shape { Circle(int), Rect(width: int, height: int), Empty }
        fun area(s: Shape): int {
            return switch (s) {
                Circle(r)  => r * r,
                Rect(w, h) => w * h,
                Empty      => 0,
            };
        }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(
        count, 1,
        "the switch-expression function should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("define internal i32 @area("),
        "missing switch-expression function:\n{}",
        c
    );
}

#[test]
fn test_hir_switch_or_pattern_expands_to_multi_const_arms() {
    // A char or-pattern expands into one `HPattern::Const` arm per alternative on the Switch path.
    let code = "
        fun is_vowel(c: char): bool {
            return switch (c) {
                'a' | 'e' | 'i' | 'o' | 'u' => true,
                _ => false,
            };
        }
    ";
    compile_test_pipeline(code, |hir, _| {
        let f = hir
            .functions
            .iter()
            .find(|f| f.name == "is_vowel")
            .expect("is_vowel should be emitted");
        let switch = f.body.iter().find_map(|s| match s {
            dream_hir::HStmt::Switch { arms, .. } => Some(arms),
            _ => None,
        });
        let arms = switch.expect("or-pattern switch should emit HStmt::Switch");
        assert_eq!(
            arms.len(),
            5,
            "five vowel alternatives should become five Const arms, got {:?}",
            arms.len()
        );
        assert!(
            arms.iter()
                .all(|a| matches!(a.pattern, dream_hir::HPattern::Const(_)))
        );
    });
}

#[test]
fn test_hir_switch_range_pattern_expands_to_multi_const_arms() {
    // A small int range expands into one Const arm per inclusive value on the Switch path.
    let code = "
        fun in_teens(n: int): bool {
            return switch (n) {
                10..12 => true,
                _ => false,
            };
        }
    ";
    compile_test_pipeline(code, |hir, _| {
        let f = hir
            .functions
            .iter()
            .find(|f| f.name == "in_teens")
            .expect("in_teens should be emitted");
        let switch = f.body.iter().find_map(|s| match s {
            dream_hir::HStmt::Switch { arms, .. } => Some(arms),
            _ => None,
        });
        let arms = switch.expect("range-pattern switch should emit HStmt::Switch");
        assert_eq!(
            arms.len(),
            3,
            "10..12 should expand to three Const arms, got {}",
            arms.len()
        );
        assert!(
            arms.iter()
                .all(|a| matches!(a.pattern, dream_hir::HPattern::Const(_)))
        );
    });
}

#[test]
fn test_switch_nested_patterns_are_exhaustive() {
    // Nested union patterns are counted recursively: `Wrap(A(n))` + `Wrap(B)` together cover the
    // `Wrap` variant (all of `Inner`), so with `Bare` the switch is exhaustive without a `_` arm.
    let code = "
        enum Inner { A(v: int), B }
        enum Outer { Wrap(inner: Inner), Bare }
        fun describe(o: Outer): int {
            return switch (o) {
                Wrap(A(n)) => n,
                Wrap(B)    => -1,
                Bare       => 0,
            };
        }
    ";
    let diagnostics = analyze_code(code);
    assert!(
        !diagnostics.has_errors(),
        "nested patterns should be exhaustive: {:?}",
        diagnostics.diagnostics
    );
}

#[test]
fn test_switch_nested_patterns_incomplete_is_rejected() {
    // Missing an inner variant (`Wrap(C)`) leaves `Wrap` only partially covered, so the switch is
    // still non-exhaustive and must be reported.
    let code = "
        enum Inner { A(v: int), B, C }
        enum Outer { Wrap(inner: Inner), Bare }
        fun describe(o: Outer): int {
            return switch (o) {
                Wrap(A(n)) => n,
                Wrap(B)    => -1,
                Bare       => 0,
            };
        }
    ";
    let diagnostics = analyze_code(code);
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Non-exhaustive switch")),
        "partial nested coverage should be non-exhaustive: {:?}",
        diagnostics.diagnostics
    );
}

#[test]
fn test_hir_emission_async_await() {
    // Async bodies emit with `Await` nodes plus a synthesized `poll_<name>` resumption; an async
    // call carries a `Future` return type.
    let code = "
        async fun delay(): void { }
        async fun work(n: int): int { delay().await; return n; }
    ";
    let (c, count) = emit_hir_to_ir(code);
    assert_eq!(count, 2, "both async functions should be emitted:\n{}", c);
    assert!(
        c.contains("define internal ptr @work(") && c.contains("define internal i32 @poll_work("),
        "missing async function / poll companion:\n{}",
        c
    );
}

#[test]
fn test_async_emits_scheduler_runtime_and_poll() {
    let code = format!(
        "{ASYNC_STUB}
        async fun delay(): void {{ Time.sleep(0).await; }}
        async fun main(): void {{ delay().await; }}"
    );
    let c = emit_hir_to_module(&code);
    assert!(c.contains("dream_run_loop"), "scheduler missing:\n{}", c);
    assert!(
        c.contains("define internal i32 @poll_delay("),
        "poll fn missing:\n{}",
        c
    );
    assert!(
        c.contains("dream_new_future("),
        "constructor missing:\n{}",
        c
    );
    assert!(c.contains("dream_await("), "suspend missing:\n{}", c);
    assert!(
        c.contains("main_dream") && c.contains("poll_main_dream"),
        "async main wrapper missing:\n{}",
        c
    );
}

#[cfg(feature = "native")]
#[test]
fn exec_async_sleep_and_await() {
    let code = format!(
        "{ASYNC_STUB}
        async fun get(): int {{
            Time.sleep(0).await;
            return 42;
        }}
        async fun main(): void {{
            let v = get().await;
            System.print(v);
        }}"
    );
    assert_eq!(run_and_capture(&code, "main"), "42");
}

#[test]
fn test_interface_call_emits_dynamic_dispatch() {
    let code = "
        interface Animal { fun speak(): string; }
        class Cat : Animal { public fun speak(): string { return \"meow\"; } }
        fun describe(a: Animal): string { return a.speak(); }
        fun run(): string { return describe(Cat()); }
    ";
    let (c, _) = emit_hir_to_ir(code);
    assert!(
        c.contains("__iface_dispatch_"),
        "interface call should dispatch through a trampoline:\n{}",
        c
    );
}

#[test]
fn test_js_desugars_to_host_bridges() {
    // Dynamic js operations desugar to the declared host bridge symbols (`jsGlobal`, ...) and fused
    // slot/method marshalling goes through `dream_js_call`. (The WAT backend's `$js_set_slot` /
    // shadow-stack plumbing has no C counterpart — arguments marshal through the same call.)
    let code = format!(
        "{JS_STUB}
        fun entry(): void {{
            let doc = js.global(\"document\");
            let el = doc.getElementById(\"app\");
            el.textContent = \"hello\";
        }}"
    );
    let (c, _count) = emit_hir_to_ir(&code);
    assert!(c.contains("jsGlobal("), "js.global:\n{}", c);
    let entry = ir_func_body(&c, "entry");
    assert!(
        entry.contains("dream_js_call("),
        "js.call / slot write:\n{}",
        c
    );
    assert!(
        !entry.contains("jsString("),
        "set_slot should not pre-box string:\n{}",
        c
    );
}

#[test]
fn test_js_fuses_get_as_string_at_typed_boundary() {
    let code = format!(
        "{JS_STUB}
        fun entry(): void {{
            let config = js.global(\"appConfig\");
            let title: string = config.title;
        }}"
    );
    let (c, _count) = emit_hir_to_ir(&code);
    let entry = ir_func_body(&c, "entry");
    assert!(entry.contains("jsGetAsString("), "fused get+unbox:\n{}", c);
    assert!(
        !entry.contains("jsGetV("),
        "should not emit plain js.get:\n{}",
        c
    );
    assert!(
        !entry.contains("jsAsString("),
        "entry should not emit separate as_string:\n{}",
        c
    );
}

#[test]
fn test_js_fuses_get_call_chain() {
    let code = format!(
        "{JS_STUB}
        fun entry(): void {{
            let config = js.global(\"appConfig\");
            let shout = config.title.toUpperCase();
        }}"
    );
    let (c, _count) = emit_hir_to_ir(&code);
    let entry = ir_func_body(&c, "entry");
    assert!(entry.contains("dream_js_call("), "fused get+call:\n{}", c);
    assert!(
        !entry.contains("jsGetV("),
        "should not emit separate js.get:\n{}",
        c
    );
}

#[test]
fn test_js_fuses_get_call_as_string() {
    let code = format!(
        "{JS_STUB}
        fun entry(): void {{
            let config = js.global(\"appConfig\");
            let shout: string = config.title.toUpperCase();
        }}"
    );
    // Native routes every js call through `dream_js_call`; wasm32 calls the fused bridge by name.
    let c = emit_hir_to_module_wasm32(&code);
    assert!(
        c.contains("@jsGetCallAsString("),
        "fused get+call+unbox:\n{}",
        c
    );
}

#[test]
fn test_js_to_value_struct_fills_in_place() {
    // A `js` -> value `struct` cast fills the destination in place: the payload is memcpy'd from the
    // js handle straight into the stack struct (no `dream_malloc` result, no separate filler call).
    let code = format!(
        "{JS_STUB}
        struct Point {{
            public x: int;
            public y: int;
        }}
        fun entry(): int {{
            let p: Point = js.global(\"origin\");
            return p.x + p.y;
        }}"
    );
    let c = emit_hir_to_module(&code);
    let entry = ir_func_body(&c, "entry");
    assert!(
        entry.contains("@llvm.memcpy.p0.p0.i64("),
        "value-struct js_to must fill the destination in place:\n{}",
        c
    );
    assert!(
        !entry.contains("dream_malloc("),
        "value-struct js_to must not allocate a heap pointer:\n{}",
        c
    );
}
