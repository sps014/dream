//! HIR->MIR->C emission and native execution tests.
//! Moved out of `dream-sema` so the analyzer crate has no `dream-mir` dependency.

mod common;
use common::*;
use dream_diagnostics::DiagnosticBag;
use dream_sema::analyzer::Analyzer;
use dream_syntax::lexer::Lexer;
use dream_syntax::parser::Parser;
use pretty_assertions::assert_eq;

/// Extracts the body of the C function `name` (the definition whose signature line ends with '{'),
/// up to its closing brace. Returns "" when no definition is found. Declarations (`;`) and
/// same-prefix functions (`poll_work` when searching `work`) are skipped.
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
fn test_hir_emission_arithmetic_function() {
    // A plain free function over arithmetic on parameters is fully representable in HIR, so the
    // analyzer emits it and it survives the MIR backend pipeline.
    let (c, count) = emit_hir_to_c("fun add(a: int, b: int): int { return a + b; }");
    assert_eq!(
        count, 1,
        "the single free function should be emitted as HIR"
    );
    assert!(c.contains("int32_t add("), "missing emitted function:\n{}", c);
    let body = c_func_body(&c, "add");
    assert!(body.contains('+'), "missing arithmetic:\n{}", c);
}

#[test]
fn test_hir_emission_locals_and_assignment() {
    // `let` + assignment + return over locals: each statement is supported, so the function emits.
    let code = "fun calc(n: int): int { let x: int = n; let y: int = x + 1; y = y + n; return y; }";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1);
    assert!(c.contains("int32_t calc("), "missing emitted function:\n{}", c);
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
    let (_, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 1,
        "only the fully-supported function should be emitted"
    );
}

#[test]
fn test_hir_emission_while_loop() {
    // `while` over locals is now fully representable; the whole function survives the pipeline.
    // Control flow is plain C now (labelled blocks + gotos), so only the comparison shape and the
    // presence of the back-edge are asserted.
    let code = "fun count(n: int): int { let s: int = 0; while (s < n) { s = s + 1; } return s; }";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the while function should be emitted as HIR");
    assert!(c.contains("int32_t count("), "missing emitted function:\n{}", c);
    let body = c_func_body(&c, "count");
    assert!(body.contains('<'), "missing loop comparison:\n{}", c);
    assert!(body.contains("goto L"), "missing CFG back-edge:\n{}", c);
}

#[test]
fn test_hir_emission_if_else_chain() {
    // `if` / `else if` / `else` folds into nested HIR `If`s and lowers to a branching CFG.
    let code = "
        fun classify(n: int): int {
            if (n < 0) { return 0; } else if (n == 0) { return 1; } else { return 2; }
        }
    ";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 1,
        "the if/else-if/else function should be emitted as HIR"
    );
    assert!(
        c.contains("int32_t classify("),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the for-loop function should be emitted as HIR");
    assert!(c.contains("int32_t sum("), "missing emitted function:\n{}", c);
    let body = c_func_body(&c, "sum");
    assert!(body.contains('+'), "missing arithmetic:\n{}", c);
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the foreach function should be emitted as HIR");
    assert!(
        c.contains("int32_t total("),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 1,
        "the logical/ternary function should be emitted as HIR"
    );
    assert!(
        c.contains("int32_t pick("),
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
    let (c, count) = emit_hir_to_c(code);
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
    // A numeric widening cast lowers to a concrete C cast expression.
    let code = "fun widen(x: int): double { return (double)x; }";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the cast function should be emitted as HIR");
    let body = c_func_body(&c, "widen");
    assert!(body.contains("(double)"), "missing widening cast:\n{}", c);
}

#[test]
fn test_hir_emission_index_and_array_literal() {
    // Array literals allocate via `dream_malloc` and store the length + elements; indexing reads
    // through the element address.
    let code = "
        fun first(xs: int[]): int { return xs[0]; }
        fun make(): int[] { return [1, 2, 3]; }
    ";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 2,
        "both the index and array-literal functions should be emitted"
    );
    assert!(
        c.contains("int32_t first("),
        "missing index function:\n{}",
        c
    );
    assert!(
        c.contains("make(void)"),
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
    let (c, _count) = emit_hir_to_c(code);
    assert!(
        c.contains("make(void)"),
        "return-context empty array should emit:\n{}",
        c
    );
    assert!(
        c.contains("int32_t driver("),
        "assignment/arg empty array should emit:\n{}",
        c
    );
    assert!(
        c.contains("Bag_constructor("),
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
    let (c, _count) = emit_hir_to_c(code);
    assert!(
        c.contains("int32_t driver("),
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
    // A direct free-function call resolves to the callee's `DefId` and emits a direct C call.
    let code = "
        fun addup(a: int, b: int): int { return a + b; }
        fun driver(): int { return addup(1, 2); }
    ";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 2, "both the callee and the caller should be emitted");
    assert!(c.contains("int32_t driver("), "missing caller:\n{}", c);
    assert!(
        c.contains("addup(1, 2)"),
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
    let (c, _count) = emit_hir_to_c(code);
    assert!(
        c.contains("int32_t Point_getx("),
        "extend method body should emit:\n{}",
        c
    );
    assert!(
        c.contains("Point_getx(p)"),
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
    let (c, _count) = emit_hir_to_c(code);
    assert!(
        c.contains("int32_t Box_int_peek("),
        "generic extend method should emit:\n{}",
        c
    );
    assert!(
        c.contains("Box_int_peek(b)"),
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
    let (c, _count) = emit_hir_to_c(code);
    assert!(
        c.contains("void Res_del("),
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
        @allow_cycle
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
        c.contains("static void release_Node("),
        "per-type release missing:\n{}",
        c
    );
    assert!(
        c.contains("Node_del(p);"),
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
        c.contains("static void destroy_object("),
        "tag-dispatch router missing:\n{}",
        c
    );
    assert!(
        c.contains("dream_recycle(p);"),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 2,
        "both the constructor body and make should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("void Point_constructor("),
        "constructor body should emit:\n{}",
        c
    );
    assert!(
        c.contains("dream_malloc("),
        "construction should allocate:\n{}",
        c
    );
    assert!(
        c.contains("Point_constructor(t0, 1, 2)"),
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
    let (c, count) = emit_hir_to_c(code);
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
        c.contains("void Box_int_constructor("),
        "the monomorphized constructor should emit:\n{}",
        c
    );
    let read = c_func_body(&c, "read");
    assert!(
        read.contains("*(int32_t *)"),
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
    let (c, _count) = emit_hir_to_c(code);
    assert!(
        c.contains("int32_t Box_int_get("),
        "generic-struct method body should emit under its mangled name:\n{}",
        c
    );
    assert!(
        c.contains("Box_int_get(b)"),
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
    let mut analyzer = Analyzer::new(&tree, &arena);
    let hir = analyzer
        .analyze(&mut diagnostics)
        .expect("analysis should succeed")
        .hir;
    assert!(!diagnostics.has_errors(), "unexpected analysis errors");

    let interner = analyzer.interner();
    let mir = dream_mir::lower::lower_program(&hir, interner);
    let c = dream_mir::backend::c::emit_c_module(&mir, interner);
    assert!(
        c.contains("void __dream_init(void)"),
        "missing init function:\n{}",
        c
    );
    // `__dream_init` is itself invoked from `dream_runtime_init`, which the C `main` calls before
    // running any user code.
    assert!(
        c.contains("__dream_init();"),
        "init must be invoked from the runtime-init wrapper:\n{}",
        c
    );
    // `g0` is the synthetic `__closure_env` global; `counter` is the first user global, so it lands
    // at `g1`.
    assert!(
        c.contains("g1 = 40"),
        "init should store the global:\n{}",
        c
    );
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
        c.contains("void log_it(int32_t a0);"),
        "extern should declare its @js target:\n{}",
        c
    );
    assert!(
        c.contains("log_it(7)"),
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
    // A defaulted extern (no `@js`) binds to a plain C symbol of the same name.
    let code = "
        extern fun now(): int;
        fun t(): int { return now(); }
    ";
    let c = emit_hir_to_module(code);
    assert!(
        c.contains("int32_t now(void);"),
        "defaulted extern should declare its result-bearing symbol:\n{}",
        c
    );
    assert!(c.contains("now()"), "call should resolve to the symbol:\n{}", c);
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
        c.contains("print_int((int32_t)41)"),
        "print(int) should call print_int:\n{}",
        c
    );
    assert!(
        c.contains("print_char(10)"),
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
        c.contains("print_string(__ds"),
        "print(string) should call print_string:\n{}",
        c
    );
    assert!(
        c.contains("{104, 105}"),
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
        assert!(
            c.contains(helper),
            "missing {helper} in print:\n{}",
            c
        );
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
        c.contains("void run("),
        "an object print should be covered now:\n{}",
        c
    );
    assert!(
        c.contains("Box_to_string(b)"),
        "object print routes through the generated to_string:\n{}",
        c
    );
    assert!(
        c.contains("dream_ptr Box_to_string(dream_ptr p)"),
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
        @allow_cycle
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
    let add_def = DefId(10);

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
    mb.set_def(DefId(11), vec![]);
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
    let c = dream_mir::backend::c::emit_c_module(&mir, &interner);
    assert!(
        c.contains("static void * dream_ft["),
        "function table missing:\n{}",
        c
    );
    assert!(
        c.contains("(void *)add"),
        "callee must be registered in the table:\n{}",
        c
    );
    assert!(
        c.contains("dream_fn_i32_i32__i32"),
        "indirect-call pointer signature missing:\n{}",
        c
    );
    assert!(
        c.contains("dream_ft["),
        "indirect call through the table missing:\n{}",
        c
    );
    assert!(
        c.contains("dream_ft_get"),
        "table accessor missing:\n{}",
        c
    );
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
    let main_body = c_func_body(&c, "main_dream");
    assert!(
        main_body.contains("dream_ft["),
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
    let mut analyzer = Analyzer::new(&tree, &arena);
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
                Statement::Retain(o) | Statement::Release(o) | Statement::ReleaseUnique(o) => {
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 3,
        "two id instances + driver should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("id__0(") && c.contains("id__7("),
        "each monomorphization gets its own symbol:\n{}",
        c
    );
    assert!(
        c.contains("id__0(5)") && c.contains("id__7(1)"),
        "each generic call site should resolve to an instance symbol:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_string_literal() {
    // A string literal resolves to its interned static data block (`__ds<N>`), laid out after the
    // runtime's own constants.
    let code = "fun greet(): string { return \"hi\"; }";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 1,
        "the string-returning function should be emitted as HIR"
    );
    assert!(
        c.contains("greet(void)"),
        "missing emitted function:\n{}",
        c
    );
    let greet = c_func_body(&c, "greet");
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 3,
        "the field-read, constructor, and constructor-body functions should be emitted"
    );
    assert!(
        c.contains("int32_t getx("),
        "missing field-read function:\n{}",
        c
    );
    assert!(
        c.contains("make(void)"),
        "missing constructor function:\n{}",
        c
    );
    // `p.x` (field 0) lowers to a real load now that the layout is threaded through.
    let getx = c_func_body(&c, "getx");
    assert!(
        getx.contains("*(int32_t *)"),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the field-assignment function should be emitted");
    assert!(
        c.contains("void bump("),
        "missing field-assignment function:\n{}",
        c
    );
    // `c.n = ...` lowers to a real store through the field address.
    let bump = c_func_body(&c, "bump");
    assert!(
        bump.contains("*(int32_t *)"),
        "field write should lower to a store:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_index_assignment() {
    // Indexed assignment lowers to an `Assign` with an `Index` place.
    let code = "fun setfirst(xs: int[], v: int): void { xs[0] = v; }";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the index-assignment function should be emitted");
    assert!(
        c.contains("void setfirst("),
        "missing index-assignment function:\n{}",
        c
    );
    // `xs[0] = v` computes the element address (base + 4 + i*stride) and stores.
    let setfirst = c_func_body(&c, "setfirst");
    assert!(
        setfirst.contains("*(int32_t *)"),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the enum-returning function should be emitted");
    assert!(
        c.contains("int32_t pick("),
        "missing enum function:\n{}",
        c
    );
    // `Color.Green` is the second member, value 1.
    let pick = c_func_body(&c, "pick");
    assert!(pick.contains("return 1"), "missing enum constant:\n{}", c);
}

#[test]
fn test_hir_emission_method_body_and_instance_call() {
    // A method body (with a `this` receiver and a field read) is emitted under its mangled name,
    // and a resolved instance-method call lowers to a direct call.
    let code = "
        class Box { public v: int; public fun get(): int { return this.v; } }
        fun use_box(b: Box): int { return b.get(); }
    ";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 2,
        "both the method body and its caller should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("int32_t Box_get("),
        "missing emitted method body:\n{}",
        c
    );
    assert!(
        c.contains("int32_t use_box("),
        "missing instance-call function:\n{}",
        c
    );
    assert!(
        c.contains("Box_get(b)"),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 2,
        "both the static method and its caller should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("int32_t M_id("),
        "missing emitted static method:\n{}",
        c
    );
    assert!(
        c.contains("M_id(7)"),
        "static call should dispatch to the method:\n{}",
        c
    );
}

#[test]
fn test_hir_emission_global_read_and_write() {
    // A module-global resolves to a C file-scope global for both reads and assignments.
    let code = "
        let counter: int = 0;
        fun tick(): int { counter = counter + 1; return counter; }
    ";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 1,
        "the global-using function should be emitted:\n{}",
        c
    );
    // `g0` is the synthetic `__closure_env` global; `counter` is the first user global, so it lands
    // at `g1`.
    let tick = c_func_body(&c, "tick");
    assert!(
        tick.contains("g1 = g1 + 1"),
        "missing global read+write:\n{}",
        c
    );
    assert!(tick.contains("return g1"), "missing global read:\n{}", c);
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 2,
        "both union constructors should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("mk(void)"),
        "missing data-variant constructor:\n{}",
        c
    );
    assert!(
        c.contains("nil(void)"),
        "missing unit-variant constructor:\n{}",
        c
    );
    // The first word of the union block is the variant discriminant.
    let mk = c_func_body(&c, "mk");
    let nil = c_func_body(&c, "nil");
    assert!(
        mk.contains("= (int32_t)0"),
        "data variant should store discriminant 0:\n{}",
        c
    );
    assert!(
        nil.contains("= (int32_t)1"),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the switch function should be emitted:\n{}", c);
    assert!(
        c.contains("int32_t classify("),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 1, "the switch function should be emitted:\n{}", c);
    assert!(
        c.contains("int32_t describe("),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 2, "both size functions should be emitted:\n{}", c);
    assert!(
        c.contains("int32_t count("),
        "missing array-len function:\n{}",
        c
    );
    assert!(
        c.contains("int32_t slen("),
        "missing string-len function:\n{}",
        c
    );
    let slen = c_func_body(&c, "slen");
    assert!(
        slen.contains("dream_str_len("),
        "string len should call dream_str_len:\n{}",
        c
    );
    let count_body = c_func_body(&c, "count");
    assert!(
        count_body.contains("*(int32_t *)dream_p(xs)"),
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
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(
        count, 1,
        "the switch-expression function should be emitted:\n{}",
        c
    );
    assert!(
        c.contains("int32_t area("),
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
        assert!(arms
            .iter()
            .all(|a| matches!(a.pattern, dream_hir::HPattern::Const(_))));
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
        assert!(arms
            .iter()
            .all(|a| matches!(a.pattern, dream_hir::HPattern::Const(_))));
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
        async fun work(n: int): int { await delay(); return n; }
    ";
    let (c, count) = emit_hir_to_c(code);
    assert_eq!(count, 2, "both async functions should be emitted:\n{}", c);
    assert!(
        c.contains("dream_ptr work(") && c.contains("int32_t poll_work("),
        "missing async function / poll companion:\n{}",
        c
    );
}

#[test]
fn test_async_emits_scheduler_runtime_and_poll() {
    let code = format!(
        "{ASYNC_STUB}
        async fun delay(): void {{ await Time.sleep(0); }}
        async fun main(): void {{ await delay(); }}"
    );
    let c = emit_hir_to_module(&code);
    assert!(
        c.contains("dream_run_loop"),
        "scheduler missing:\n{}",
        c
    );
    assert!(
        c.contains("int32_t poll_delay("),
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
            await Time.sleep(0);
            return 42;
        }}
        async fun main(): void {{
            let v = await get();
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
    let (c, _) = emit_hir_to_c(code);
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
    let (c, _count) = emit_hir_to_c(&code);
    assert!(c.contains("jsGlobal("), "js.global:\n{}", c);
    let entry = c_func_body(&c, "entry");
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
    let (c, _count) = emit_hir_to_c(&code);
    let entry = c_func_body(&c, "entry");
    assert!(
        entry.contains("jsGetAsString("),
        "fused get+unbox:\n{}",
        c
    );
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
    let (c, _count) = emit_hir_to_c(&code);
    let entry = c_func_body(&c, "entry");
    assert!(
        entry.contains("dream_js_call("),
        "fused get+call:\n{}",
        c
    );
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
    let (c, _count) = emit_hir_to_c(&code);
    assert!(
        c.contains("jsGetCallAsString("),
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
    let entry = c_func_body(&c, "entry");
    assert!(
        entry.contains("memcpy(dream_p("),
        "value-struct js_to must fill the destination in place:\n{}",
        c
    );
    assert!(
        !entry.contains("dream_malloc("),
        "value-struct js_to must not allocate a heap pointer:\n{}",
        c
    );
}
