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
        borrowed_result: false,
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
fn hir_to_ir_pipeline_emits_expected_shape_deterministically() {
    let ll = compile_sum_to();
    assert_eq!(
        ll,
        compile_sum_to(),
        "the new backend pipeline must be byte-for-byte deterministic"
    );
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

/// `checked { }` covers compound assignment, `++` on unsigned operands, and lambdas written in the
/// block even when they are called outside it; arithmetic outside the block stays unchecked.
#[cfg(feature = "native")]
#[test]
fn checked_blocks_emit_overflow_intrinsics() {
    let ir = emit_opt_ll(
        r#"
        fun compound(a: int): int { let x = a; checked { x += 1; } return x; }
        fun increment(a: uint): uint { let x = a; checked { x++; } return x; }
        fun make_times(): fun(int): int {
            let times: fun(int): int = (v) => v;
            checked {
                times = (v) => v * 1000000000;
            }
            return times;
        }
        fun unchecked_mul(a: int): int { return a * 1000000000; }
        fun main(): void {
            let times = make_times();
            let total = compound(1) + times(7) + unchecked_mul(2);
            let more = increment(1u);
        }
        "#,
        &["--emit-llvm"],
    );
    let compound = common::ir_func_body(&ir, "compound");
    assert!(
        compound.contains("@llvm.sadd.with.overflow.i32("),
        "checked `+=` lost its overflow check:\n{compound}"
    );
    let increment = common::ir_func_body(&ir, "increment");
    assert!(
        increment.contains("@llvm.uadd.with.overflow.i32("),
        "checked `++` on uint lost its overflow check:\n{increment}"
    );
    let unchecked = common::ir_func_body(&ir, "unchecked_mul");
    assert!(
        !unchecked.contains("with.overflow"),
        "arithmetic outside `checked` must wrap:\n{unchecked}"
    );
    assert!(
        ir.contains("call { i32, i1 } @llvm.smul.with.overflow.i32("),
        "the lambda written in `checked` lost its overflow check:\n{ir}"
    );
}

/// Compiles `source` with `--release --emit-llvm` and returns the LLVM-optimized module.
#[cfg(feature = "native")]
fn release_opt_ll(source: &str) -> String {
    emit_opt_ll(source, &["--release", "--emit-llvm"])
}

#[cfg(feature = "native")]
fn emit_opt_ll(source: &str, args: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("spans.dream");
    let out = dir.path().join("out/spans.ll");
    std::fs::write(&src, source).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dream"))
        .env("NO_COLOR", "1")
        .args(args)
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read_to_string(out.with_extension("opt.ll")).unwrap()
}

/// Spans over borrowed and private owned sources, value-struct constructors and borrowed
/// value parameters all scalarize: no RC in the loops, bounds checks fold, and only the
/// private source owner keeps its cleanup.
#[cfg(feature = "native")]
#[test]
fn spans_and_value_structs_scalarize_without_reference_counting() {
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
        @noinline
        fun owned_span(borrow suffix: string, n: int): int {
            let source = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789" + suffix;
            let sum = 0;
            let i = 0;
            while i < n {
                let start = i & 7;
                let view = source.span(start, start + 25 + (i & 3));
                sum = sum + view.length + (int)view[0];
                i = i + 1;
            }
            return sum;
        }
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
        fun borrowed_string(borrow view: ReadOnlySpan<char>): int {
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
            System.println(span_len("hello world, this is a fairly long literal for spans", 100));
            let a = [1, 2, 3, 4, 5];
            System.println(sum_span(Span.of(a)));
            System.println(owned_span(System.env("DREAM_SPAN_INPUT").unwrap_or("!"), 100));
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
    let span_len = common::ir_func_body(&ir, "span_len");
    for needle in ["alloca", "atomic"] {
        assert!(
            !span_len.contains(needle),
            "`{needle}` left in span_len:\n{span_len}"
        );
    }
    assert!(
        !span_len
            .lines()
            .any(|l| l.contains("call ") && !l.contains("@llvm.")),
        "runtime call left in span_len:\n{span_len}"
    );
    let sum_span = common::ir_func_body(&ir, "sum_span");
    assert!(
        sum_span.contains("vector.body"),
        "sum_span did not vectorize:\n{sum_span}"
    );
    {
        let body = common::ir_func_body(&ir, "owned_span");
        assert_eq!(
            (
                body.matches("@dream_rc_last_slow(").count(),
                body.matches("@dream_rc_last_observed(").count()
            ),
            (1, 1),
            "only the observed/unobserved branches of the source owner's cleanup may remain:\n{body}"
        );
        assert_eq!(
            body.matches("@dream_free(").count(),
            1,
            "one source owner: {body}"
        );
        assert!(
            !body.contains("atomicrmw"),
            "a span retained its private source:\n{body}"
        );
    }
    for name in [
        "array_view",
        "readonly_view",
        "struct_view",
        "borrowed_array",
        "borrowed_string",
        "borrowed_struct",
        "span_equals",
        "string_equals",
    ] {
        let body = common::ir_func_body(&ir, name);
        for operation in [
            "atomic",
            "@dream_retain",
            "@dream_release",
            "@dream_malloc",
            "@dream_str_sub",
        ] {
            assert!(
                !body.contains(operation),
                "{operation} left in {name}:\n{body}"
            );
        }
        assert!(
            !body.contains("alloca"),
            "value did not scalarize in {name}:\n{body}"
        );
    }
    for name in ["array_view", "readonly_view", "struct_view"] {
        let body = common::ir_func_body(&ir, name);
        assert!(
            body.contains("dream_panic"),
            "constructor validation lost in {name}:\n{body}"
        );
    }
}

#[test]
fn source_weak_forest_builder_batches_niche_null_initialization() {
    let fixture = include_str!("fixtures/compiler/arc_weak_forest.dream");
    let (_, fixture) = fixture.split_once("class ForestNode").unwrap();
    let (fixture, _) = fixture.split_once("fun main()").unwrap();
    let source = format!(
        "enum Option<T> {{ Some(T), None }} @allow_cycle class ForestNode{fixture} fun main(): void {{ let root = forest(5); }}"
    );
    common::compile_test_pipeline(&source, |hir, interner| {
        let mut mir = lower_program(hir, interner);
        let mut dump = dream_mir::passes::MirDump::default();
        dream_mir::passes::optimize_module_opts(&mut mir, interner, true, &mut dump);
        dream_mir::passes::run_function_pipelines(
            &mut mir,
            interner,
            &PassManager::release_pipeline(),
            &PassManager::async_poll_pipeline(),
            &mut dump,
        );
        dream_mir::passes::run_late_module_passes(&mut mir, interner, &mut dump);
        let forest = mir
            .functions
            .iter()
            .find(|function| function.name == "forest")
            .unwrap();
        assert!(
            forest
                .blocks
                .iter()
                .flat_map(|b| &b.stmts)
                .any(|statement| {
                    matches!(
                        statement,
                        dream_mir::Statement::Assign(_, dream_mir::Rvalue::New { ctor: None, .. })
                    )
                })
        );
        let ir = common::emit_ll(&mir, interner);
        let ordinary = common::ir_func_body(&ir, "forest");
        assert!(!ordinary.contains("dream_cycle_"));
    });
}

#[test]
fn recursive_builder_uses_plain_arc_without_collector_clones() {
    let source = r#"
        enum Option<T> { Some(T), None }
        @allow_cycle
        class Tree {
            public left: Option<Tree>;
            public right: Option<Tree>;
            public constructor(left: Option<Tree>, right: Option<Tree>) {
                this.left = left; this.right = right;
            }
        }
        fun build_tree(depth: int): Option<Tree> {
            if depth == 0 { return Option.None; }
            return Option.Some(Tree(build_tree(depth - 1), build_tree(depth - 1)));
        }
        fun count(tree: Option<Tree>): int {
            return switch (tree) {
                Some(node) => 1 + count(node.left) + count(node.right),
                None => 0,
            };
        }
        fun main(): int { return count(build_tree(5)); }
    "#;
    common::compile_test_pipeline(source, |hir, interner| {
        let mut mir = lower_program(hir, interner);
        mir.profile = dream_abi::profile::CompileProfile::Release;
        let mut dump = dream_mir::passes::MirDump::disabled();
        dream_mir::passes::optimize_module_opts(&mut mir, interner, true, &mut dump);
        dream_mir::passes::run_function_pipelines(
            &mut mir,
            interner,
            &PassManager::release_pipeline(),
            &PassManager::async_poll_pipeline(),
            &mut dump,
        );
        dream_mir::passes::run_late_module_passes(&mut mir, interner, &mut dump);
        let ir = common::emit_ll(&mir, interner);
        let body = common::ir_func_body(&ir, "build_tree");
        assert!(!body.contains("dream_cycle_"));
        assert!(!ir.contains("__tracked_graph"));
        assert!(!ir.contains("__private_graph"));
        assert!(body.contains("@build_tree("));
    });
}

#[test]
fn typed_slot_swap_emits_checks_without_arc_transitions() {
    use dream_mir::{Operand, Place, Statement, Terminator};
    let mut ctx = TypeCtx::new();
    let string = ctx.interner.string();
    let arr = ctx.interner.array(string);
    let mut b = dream_mir::build::FunctionBuilder::new("swap_slots", ctx.interner.void());
    let a = b.new_param(arr, None);
    let other = b.new_param(arr, None);
    b.push(Statement::ArraySlotSwap {
        elem_ty: string,
        array: Operand::Copy(Place::Local(a)),
        left: Operand::Const(dream_mir::Const::Int(0)),
        other_array: Operand::Copy(Place::Local(other)),
        right: Operand::Const(dream_mir::Const::Int(1)),
    });
    b.terminate(Terminator::Return(None));
    let mir = dream_mir::Mir {
        functions: vec![b.finish()],
        ..Default::default()
    };
    assert!(dream_mir::verify::verify_module(&mir, &ctx.interner).is_empty());
    let ir = common::emit_ll(&mir, &ctx.interner);
    let body = common::ir_func_body(&ir, "swap_slots");
    assert_eq!(body.matches("@dream_array_at(").count(), 2);
    assert!(!body.contains("@dream_retain("));
    assert!(!body.contains("@dream_release("));
    assert_eq!(body.matches("@llvm.memcpy.p0.p0.i64(").count(), 3);
}
