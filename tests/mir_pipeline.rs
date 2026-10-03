//! End-to-end test of the backend pipeline: a hand-built typed HIR program is lowered to MIR, run
//! through the full optimization pass pipeline, and emitted as LLVM IR. This is the exact chain the
//! driver runs, so it both proves the pipeline composes and pins its determinism contract
//! (byte-identical output).

mod common;

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
            is_ref: false,
            is_take: false,
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
