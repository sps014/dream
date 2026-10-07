//! End-to-end test of the backend pipeline: a hand-built typed HIR program is lowered to MIR, run
//! through the full optimization pass pipeline, and emitted as LLVM IR. This is the exact chain the
//! driver runs, so it both proves the pipeline composes and pins its determinism contract
//! (byte-identical output).

use crate::common;

use dream_hir::{
    BinOp, Binding, HExpr, HExprKind, HFunction, HParam, HPlace, HStmt, Hir, LocalId, Overflow,
};
use dream_mir::lower::lower_program;
use dream_mir::passes::{
    ConstFold, CopyConstProp, Dce, OverflowElim, PassManager, RcElision, RcInsertion, SimplifyCfg,
};
use dream_types::{DefKind, TypeCtx};

/// Builds, lowers, optimizes, and emits the following program, returning the module text:
///
/// ```text
/// fun sum_to(n: int): int {
///     let i: int = 0;
///     let acc: int = 0;
///     while (i < n) { acc = acc + i; i = i + 1; }
///     return acc;
/// }
/// ```
///
/// A fresh `TypeCtx` is used each call so the result depends only on the pipeline, not on shared
/// interner state — which is what makes the determinism assertion meaningful.
fn compile_sum_to() -> String {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Function, "sum_to", vec![]);
    let int = ctx.interner.int();
    let boolean = ctx.interner.bool();

    let n = LocalId(0);
    let i = LocalId(1);
    let acc = LocalId(2);

    let var = |local: LocalId| HExpr::new(int, HExprKind::Var(Binding::Local(local)));

    let func = HFunction {
        def,
        name: "sum_to".into(),
        symbol: "sum_to".into(),
        instance: vec![],
        params: vec![HParam {
            local: n,
            name: "n".into(),
            ty: int,
            mode: dream_hir::ParamMode::Borrow,
        }],
        ret: int,
        locals: vec![
            dream_hir::HLocal {
                id: i,
                name: "i".into(),
                ty: int,
            },
            dream_hir::HLocal {
                id: acc,
                name: "acc".into(),
                ty: int,
            },
        ],
        is_async: false,
        file: None,
        inline: dream_hir::InlineHint::Default,
        body: vec![
            HStmt::Let {
                local: i,
                ty: int,
                value: HExpr::new(int, HExprKind::IntLit(0)),
            },
            HStmt::Let {
                local: acc,
                ty: int,
                value: HExpr::new(int, HExprKind::IntLit(0)),
            },
            HStmt::While {
                cond: HExpr::new(
                    boolean,
                    HExprKind::Binary {
                        op: BinOp::Lt,
                        lhs: Box::new(var(i)),
                        rhs: Box::new(var(n)),
                        overflow: Overflow::Checked,
                    },
                ),
                body: vec![
                    HStmt::Assign {
                        place: HPlace::Local(acc),
                        value: HExpr::new(
                            int,
                            HExprKind::Binary {
                                op: BinOp::Add,
                                lhs: Box::new(var(acc)),
                                rhs: Box::new(var(i)),
                                overflow: Overflow::Checked,
                            },
                        ),
                    },
                    HStmt::Assign {
                        place: HPlace::Local(i),
                        value: HExpr::new(
                            int,
                            HExprKind::Binary {
                                op: BinOp::Add,
                                lhs: Box::new(var(i)),
                                rhs: Box::new(HExpr::new(int, HExprKind::IntLit(1))),
                                overflow: Overflow::Checked,
                            },
                        ),
                    },
                ],
                label: None,
            },
            HStmt::Return(Some(var(acc))),
        ],
    };

    let hir = Hir {
        functions: vec![func],
        globals: vec![],
        instances: vec![],
        ..Default::default()
    };

    let mut mir = lower_program(&hir, &ctx.interner);

    // Mirror the intended production pipeline, exercising every shipped pass (RC insertion/elision
    // are no-ops here since the function is reference-free, but must still compose cleanly).
    let mut pm = PassManager::new();
    pm.add(CopyConstProp);
    pm.add(ConstFold);
    pm.add(OverflowElim);
    pm.add(SimplifyCfg);
    pm.add(Dce);
    pm.add(RcInsertion);
    pm.add(RcElision);
    for f in &mut mir.functions {
        pm.run(f, &ctx.interner);
    }

    common::emit_ll(&mir, &ctx.interner)
}

#[test]
fn hir_to_ir_pipeline_emits_expected_shape() {
    let ll = compile_sum_to();
    let body = {
        let start = ll.find("@sum_to(").expect("missing function");
        let start = ll[..start].rfind("define ").expect("sum_to is not defined");
        let len = ll[start..].find("\n}\n").expect("unterminated function");
        &ll[start..start + len]
    };
    // `acc + i` is unbounded and keeps its check; `i + 1` is bounded by `i < n` and does not.
    assert_eq!(
        body.matches("@llvm.sadd.with.overflow.i32").count(),
        1,
        "expected only the accumulator add to stay checked:\n{}",
        body
    );
    assert!(
        body.contains("add "),
        "missing the unchecked add:\n{}",
        body
    );
    assert!(
        body.contains("icmp slt"),
        "missing loop comparison:\n{}",
        body
    );
    assert!(body.contains("br i1 "), "missing loop exit:\n{}", body);
}

#[test]
fn hir_to_ir_pipeline_is_deterministic() {
    let first = compile_sum_to();
    let second = compile_sum_to();
    assert_eq!(
        first, second,
        "the new backend pipeline must be byte-for-byte deterministic"
    );
}

/// Compiles `source` with `--release --emit-llvm` and returns the LLVM-optimized module.
#[cfg(feature = "native")]
fn release_opt_ll(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("spans.dream");
    let out = dir.path().join("out/spans.ll");
    std::fs::write(&src, source).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dream"))
        .env("NO_COLOR", "1")
        .args(["--release", "--emit-llvm"])
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    std::fs::read_to_string(out.with_extension("opt.ll")).unwrap()
}

/// Spans over a borrowed source scalarize into registers with no RC, so LLVM folds `length`
/// and removes the indexer's bounds checks from the hot loop.
#[cfg(feature = "native")]
#[test]
fn spans_scalarize_and_drop_bounds_checks() {
    let ir = release_opt_ll(
        r#"
        import system;
        @noinline
        fun span_len(borrow s: string, n: int): int {
            let acc = 0;
            let i = 0;
            while i < n {
                let sp = s.span(5, 40);
                acc = acc + sp.length;
                i = i + 1;
            }
            return acc;
        }
        @noinline
        fun sum_span(sp: Span<int>): int {
            let acc = 0;
            let i = 0;
            while i < sp.length {
                acc = acc + sp[i];
                i = i + 1;
            }
            return acc;
        }
        fun main(): void {
            System.println(span_len("hello world, this is a fairly long literal for spans", 100));
            let a = [1, 2, 3, 4, 5];
            System.println(sum_span(Span.of(a)));
        }
    "#,
    );
    let span_len = common::ir_func_body(&ir, "span_len");
    for needle in ["alloca", "atomic"] {
        assert!(!span_len.contains(needle), "`{needle}` left in span_len:\n{span_len}");
    }
    assert!(
        !span_len.lines().any(|l| l.contains("call ") && !l.contains("@llvm.")),
        "runtime call left in span_len:\n{span_len}"
    );
    let sum_span = common::ir_func_body(&ir, "sum_span");
    assert!(sum_span.contains("vector.body"), "sum_span did not vectorize:\n{sum_span}");
}

#[cfg(feature = "native")]
#[test]
fn value_struct_constructors_and_borrow_parameters_elide_arc() {
    let ir = release_opt_ll(
        r#"
        import system;
        struct View {
            public source: string;
            public count: int;
            public constructor(borrow source: string, count: int) {
                if count < 0 { System.panic("invalid view"); }
                this.source = source;
                this.count = count;
            }
        }
        @noinline
        fun array_view(borrow xs: int[], start: int, count: int): int {
            let view = Span<int>(xs, start, count);
            return view.length;
        }
        @noinline
        fun readonly_view(borrow xs: int[], start: int, count: int): int {
            let view = ReadOnlySpan<int>(xs, start, count);
            return view.length;
        }
        @noinline
        fun struct_view(borrow s: string, count: int): int {
            let view = View(s, count);
            return view.count;
        }
        @noinline
        fun borrowed_array(borrow view: Span<int>): int {
            let copy = view;
            return copy.length;
        }
        @noinline
        fun borrowed_string(borrow view: StringSpan): int {
            let copy = view;
            return copy.length;
        }
        @noinline
        fun borrowed_struct(borrow view: View): int {
            let copy = view;
            return copy.count;
        }
        @noinline
        fun span_equals(borrow s: string, borrow other: string): bool {
            return s.span() == other;
        }
        @noinline
        fun string_equals(borrow s: string, borrow other: string): bool {
            return s == other.span();
        }
        fun main(): void {
            let xs = [1, 2, 3, 4];
            System.println(array_view(xs, 1, 2));
            System.println(readonly_view(xs, 1, 2));
            System.println(struct_view("hello", 3));
            System.println(borrowed_array(Span.of(xs)));
            System.println(borrowed_string("hello".span()));
            System.println(borrowed_struct(View("hello", 3)));
            System.println(span_equals("hello", "world"));
            System.println(string_equals("hello", "world"));
        }
        "#,
    );
    for name in ["array_view", "readonly_view", "struct_view", "borrowed_array", "borrowed_string", "borrowed_struct", "span_equals", "string_equals"] {
        let body = common::ir_func_body(&ir, name);
        for operation in ["atomic", "@dream_retain", "@dream_release", "@dream_malloc", "@dream_str_sub"] {
            assert!(!body.contains(operation), "{operation} left in {name}:\n{body}");
        }
        assert!(!body.contains("alloca"), "value did not scalarize in {name}:\n{body}");
    }
    for name in ["array_view", "readonly_view", "struct_view"] {
        let body = common::ir_func_body(&ir, name);
        assert!(body.contains("dream_panic"), "constructor validation lost in {name}:\n{body}");
    }
}
