use super::pipeline::Inliner;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use crate::Terminator;
use crate::build::FunctionBuilder;
use crate::passes::ModulePass;
use crate::{Const, MirFunction};
use dream_types::{DefKind, TypeCtx, TypeId};

/// Builds `fun callee(a: int): int { return a + 1; }` and `fun caller(): int { return callee(41); }`
/// and checks the call is replaced by the inlined body (no residual `Call`).
#[test]
fn inlines_small_callee() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let callee_def = ctx.register(DefKind::Function, "callee", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);

    let callee = {
        let mut b = FunctionBuilder::new("callee", int);
        b.set_def(callee_def, vec![]);
        let a = b.new_param(int, Some("a".into()));
        let t = b.new_temp(int);
        b.assign(
            Place::Local(t),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Copy(Place::Local(a)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        b.finish()
    };
    let caller = {
        let mut b = FunctionBuilder::new("caller", int);
        b.set_def(caller_def, vec![]);
        let r = b.new_temp(int);
        b.assign(
            Place::Local(r),
            Rvalue::Call {
                callee: crate::Callee {
                    def: callee_def,
                    args: vec![],
                    ret: int,
                    take_params: vec![],
                },
                args: vec![Operand::Const(Const::Int(41))],
            },
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(r)))));
        b.finish()
    };

    let mut mir = crate::Mir {
        functions: vec![callee, caller],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    let caller: &MirFunction = mir.functions.iter().find(|f| f.name == "caller").unwrap();
    let has_call = caller.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(
            s,
            Statement::Call { .. } | Statement::Assign(_, Rvalue::Call { .. })
        )
    });
    assert!(!has_call, "call to callee should have been inlined away");
}

fn fat_int_callee(
    name: &str,
    def: dream_types::DefId,
    int: TypeId,
    inline: dream_hir::InlineHint,
) -> MirFunction {
    let mut b = FunctionBuilder::new(name, int);
    b.set_def(def, vec![]);
    b.set_inline(inline);
    let a = b.new_param(int, Some("a".into()));
    let t = b.new_temp(int);
    for _ in 0..70 {
        b.assign(
            Place::Local(t),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Copy(Place::Local(a)),
                Operand::Const(Const::Int(1)),
            ),
        );
    }
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    b.finish()
}

fn caller_of(
    callee_def: dream_types::DefId,
    caller_def: dream_types::DefId,
    int: TypeId,
) -> MirFunction {
    let mut b = FunctionBuilder::new("caller", int);
    b.set_def(caller_def, vec![]);
    let r = b.new_temp(int);
    b.assign(
        Place::Local(r),
        Rvalue::Call {
            callee: crate::Callee {
                def: callee_def,
                args: vec![],
                ret: int,
                take_params: vec![],
            },
            args: vec![Operand::Const(Const::Int(41))],
        },
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(r)))));
    b.finish()
}

fn caller_still_calls(mir: &crate::Mir) -> bool {
    let caller = mir.functions.iter().find(|f| f.name == "caller").unwrap();
    caller.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(
            s,
            Statement::Call { .. } | Statement::Assign(_, Rvalue::Call { .. })
        )
    })
}

#[test]
fn prefer_inline_raises_size_budget() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let callee_def = ctx.register(DefKind::Function, "fat", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
    let mut mir = crate::Mir {
        functions: vec![
            fat_int_callee("fat", callee_def, int, dream_hir::InlineHint::Prefer),
            caller_of(callee_def, caller_def, int),
        ],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    assert!(
        !caller_still_calls(&mir),
        "flagged fat callee should inline"
    );
}

#[test]
fn unflagged_fat_callee_is_not_inlined() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let callee_def = ctx.register(DefKind::Function, "fat", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
    let mut mir = crate::Mir {
        functions: vec![
            fat_int_callee("fat", callee_def, int, dream_hir::InlineHint::Default),
            caller_of(callee_def, caller_def, int),
        ],
        ..Default::default()
    };
    assert!(!Inliner.run(&mut mir, &ctx.interner));
    assert!(
        caller_still_calls(&mir),
        "unflagged fat callee should stay a call"
    );
}

#[test]
fn noinline_small_callee_stays_a_call() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let callee_def = ctx.register(DefKind::Function, "tiny", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
    let mut b = FunctionBuilder::new("tiny", int);
    b.set_def(callee_def, vec![]);
    b.set_inline(dream_hir::InlineHint::Never);
    let a = b.new_param(int, Some("a".into()));
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(a)))));
    let mut mir = crate::Mir {
        functions: vec![b.finish(), caller_of(callee_def, caller_def, int)],
        ..Default::default()
    };
    assert!(!Inliner.run(&mut mir, &ctx.interner));
    assert!(
        caller_still_calls(&mir),
        "@noinline callee must stay a call"
    );
}

#[test]
fn inlines_small_looping_callee() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let callee_def = ctx.register(DefKind::Function, "looping", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
    let callee = {
        let mut b = FunctionBuilder::new("looping", int);
        b.set_def(callee_def, vec![]);
        let n = b.new_param(int, Some("n".into()));
        let acc = b.new_temp(int);
        let cmp = b.new_temp(int);
        b.assign(
            Place::Local(acc),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let header = b.new_block();
        let body = b.new_block();
        let done = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                crate::BinOp::Lt,
                Operand::Copy(Place::Local(acc)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk: body,
            else_blk: done,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(acc),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Copy(Place::Local(acc)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(done);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(acc)))));
        b.finish()
    };
    let mut mir = crate::Mir {
        functions: vec![callee, caller_of(callee_def, caller_def, int)],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    assert!(
        !caller_still_calls(&mir),
        "small looping callee should inline; C shape emit uses labeled gotos for multi-entry loops"
    );
}

/// Value-struct callee with an owning local: inlining inserts `ValueDrop` and marks the
/// remapped local `manual_drop` (so frame teardown will not double-drop).
#[test]
fn inlines_value_callee_with_owning_local() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let vs_def = ctx.register(DefKind::Struct, "Point", vec![]);
    ctx.defs.mark_value(vs_def);
    ctx.interner.mark_value_def(vs_def);
    let point = ctx.interner.struct_ty(vs_def, vec![]);

    let callee_def = ctx.register(DefKind::Function, "make", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);

    let callee = {
        let mut b = FunctionBuilder::new("make", int);
        b.set_def(callee_def, vec![]);
        let p = b.new_local(point, Some("p".into()));
        b.assign(Place::Local(p), Rvalue::Use(Operand::Copy(Place::Local(p))));
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(1)))));
        b.finish()
    };
    let caller = {
        let mut b = FunctionBuilder::new("caller", int);
        b.set_def(caller_def, vec![]);
        let r = b.new_temp(int);
        b.assign(
            Place::Local(r),
            Rvalue::Call {
                callee: crate::Callee {
                    def: callee_def,
                    args: vec![],
                    ret: int,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(r)))));
        b.finish()
    };

    let mut mir = crate::Mir {
        functions: vec![callee, caller],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    let caller: &MirFunction = mir.functions.iter().find(|f| f.name == "caller").unwrap();
    let has_call = caller.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(
            s,
            Statement::Call { .. } | Statement::Assign(_, Rvalue::Call { .. })
        )
    });
    assert!(
        !has_call,
        "call to value callee should have been inlined away"
    );
    assert!(
        caller
            .locals
            .iter()
            .any(|d| d.name.as_deref() == Some("p") && d.manual_drop && d.ty == point),
        "owning value local should be remapped with manual_drop"
    );
    assert!(
        caller
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(s, Statement::ValueDrop(_))),
        "inlined owning value local must get ValueDrop at the continuation"
    );
}

/// Method-style callee whose only value local is `this` (borrow): inlines with `this` as `is_ref`.
#[test]
fn inlines_this_borrow_without_value_drop() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let vs_def = ctx.register(DefKind::Struct, "Span", vec![]);
    ctx.defs.mark_value(vs_def);
    ctx.interner.mark_value_def(vs_def);
    let span = ctx.interner.struct_ty(vs_def, vec![]);

    let callee_def = ctx.register(DefKind::Function, "len", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);

    let callee = {
        let mut b = FunctionBuilder::new("len", int);
        b.set_def(callee_def, vec![]);
        let this = b.new_param(span, Some("this".into()));
        let _ = this;
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        b.finish()
    };
    let caller = {
        let mut b = FunctionBuilder::new("caller", int);
        b.set_def(caller_def, vec![]);
        let s = b.new_local(span, Some("s".into()));
        let r = b.new_temp(int);
        b.assign(
            Place::Local(r),
            Rvalue::Call {
                callee: crate::Callee {
                    def: callee_def,
                    args: vec![],
                    ret: int,
                    take_params: vec![],
                },
                args: vec![Operand::Copy(Place::Local(s))],
            },
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(r)))));
        b.finish()
    };

    let mut mir = crate::Mir {
        functions: vec![callee, caller],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    let caller: &MirFunction = mir.functions.iter().find(|f| f.name == "caller").unwrap();
    let has_call = caller.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(
            s,
            Statement::Call { .. } | Statement::Assign(_, Rvalue::Call { .. })
        )
    });
    assert!(!has_call, "call should have been inlined");
    assert!(
        caller
            .locals
            .iter()
            .any(|d| d.is_ref && d.name.as_deref() == Some("this")),
        "remapped this must stay is_ref"
    );
}

/// A directly self-recursive function must not be inlined into itself.
#[test]
fn skips_recursion() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let def = ctx.register(DefKind::Function, "rec", vec![]);
    let mut b = FunctionBuilder::new("rec", int);
    b.set_def(def, vec![]);
    let t = b.new_temp(int);
    b.assign(
        Place::Local(t),
        Rvalue::Call {
            callee: crate::Callee {
                def,
                args: vec![],
                ret: int,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    let mut mir = crate::Mir {
        functions: vec![b.finish()],
        ..Default::default()
    };
    assert!(!Inliner.run(&mut mir, &ctx.interner));
}

/// A transparent callee inlined into a retain/print/release sequence lets `RcElision` cancel the
/// pair that a call barrier would have kept.
#[test]
fn inlined_callee_lets_elision_cancel_rc_pair() {
    use crate::passes::MirPass;
    use crate::passes::rc::{RcElision, RcInsertion};

    let mut ctx = TypeCtx::new();
    let void = ctx.interner.void();
    let callee_def = ctx.register(DefKind::Function, "peek", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);

    let callee = {
        let mut b = FunctionBuilder::new("peek", void);
        b.set_def(callee_def, vec![]);
        b.terminate(Terminator::Return(None));
        b.finish()
    };
    let caller = {
        let mut b = FunctionBuilder::new("caller", void);
        b.set_def(caller_def, vec![]);
        let x = b.new_local(ctx.interner.string(), Some("x".into()));
        b.assign(
            Place::Local(x),
            Rvalue::Use(Operand::Const(Const::Str("hi".into()))),
        );
        b.push(Statement::Call {
            callee: crate::Callee {
                def: callee_def,
                args: vec![],
                ret: void,
                take_params: vec![],
            },
            args: vec![],
        });
        b.push(Statement::Print {
            arg: Operand::Copy(Place::Local(x)),
            ty: ctx.interner.string(),
            newline: true,
        });
        b.terminate(Terminator::Return(None));
        b.finish()
    };

    let mut mir = crate::Mir {
        functions: vec![callee, caller],
        ..Default::default()
    };
    for f in &mut mir.functions {
        RcInsertion.run(f, &ctx.interner);
    }
    assert!(Inliner.run(&mut mir, &ctx.interner));
    let caller_idx = mir
        .functions
        .iter()
        .position(|f| f.name == "caller")
        .unwrap();
    assert!(
        !mir.functions[caller_idx]
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(s, Statement::Call { .. })),
        "peek should be inlined"
    );
    RcElision.run(&mut mir.functions[caller_idx], &ctx.interner);
    let retains = mir.functions[caller_idx]
        .blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert_eq!(
        retains, 0,
        "inlined transparent peek should let elision drop retain of x"
    );
}

/// `return s` inlined as `x = s` must be a last-use move after fused RC, not a second alias.
#[test]
fn optimize_module_rc_after_inline_moves_returned_string() {
    let mut ctx = TypeCtx::new();
    let str_ty = ctx.interner.string();
    let void = ctx.interner.void();
    let make_def = ctx.register(DefKind::Function, "make", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
    let main_def = ctx.register(DefKind::Function, "main", vec![]);

    let make = {
        let mut b = FunctionBuilder::new("make", str_ty);
        b.set_def(make_def, vec![]);
        let s = b.new_local(str_ty, Some("s".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(Const::Str("hi".into()))),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
        b.finish()
    };
    let caller = {
        let mut b = FunctionBuilder::new("caller", void);
        b.set_def(caller_def, vec![]);
        let x = b.new_local(str_ty, Some("x".into()));
        b.assign(
            Place::Local(x),
            Rvalue::Call {
                callee: crate::Callee {
                    def: make_def,
                    args: vec![],
                    ret: str_ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        b.push(Statement::Print {
            arg: Operand::Copy(Place::Local(x)),
            ty: str_ty,
            newline: true,
        });
        b.terminate(Terminator::Return(None));
        b.finish()
    };
    let main = {
        let mut b = FunctionBuilder::new(crate::abi::ENTRY_FN, void);
        b.set_def(main_def, vec![]);
        b.push(Statement::Call {
            callee: crate::Callee {
                def: caller_def,
                args: vec![],
                ret: void,
                take_params: vec![],
            },
            args: vec![],
        });
        b.terminate(Terminator::Return(None));
        b.finish()
    };

    let mut mir = crate::Mir {
        functions: vec![make, caller, main],
        ..Default::default()
    };
    let cap_hits = crate::passes::limits::hits(crate::passes::limits::Limit::Inline);
    crate::passes::module_pipeline::optimize_module_rounds(
        &mut mir,
        &ctx.interner,
        true,
        &mut crate::passes::MirDump::disabled(),
        1,
    );
    assert_eq!(
        crate::passes::limits::hits(crate::passes::limits::Limit::Inline),
        cap_hits + 1
    );
    assert!(crate::verify::verify_module(&mir, &ctx.interner).is_empty());
    let caller = mir
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .expect("caller survives prune");
    let has_make_call = caller.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(
            s,
            Statement::Assign(_, Rvalue::Call { callee, .. }) if callee.def == make_def
        )
    });
    assert!(!has_make_call, "make should be inlined");
    let string_teardown = caller.blocks.iter().flat_map(|b| &b.stmts).any(|st| {
        matches!(
            st,
            Statement::Release(Operand::Copy(Place::Local(_)))
                | Statement::Assign(Place::Local(_), Rvalue::Use(Operand::Const(Const::Null)))
        )
    });
    assert!(
        string_teardown,
        "fused body still tears down RC locals: {:?}",
        caller.blocks.iter().map(|b| &b.stmts).collect::<Vec<_>>()
    );
}

#[test]
fn inlined_value_this_stores_into_caller_storage() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "View", vec![]);
    ctx.interner.mark_value_def(def);
    let view = ctx.interner.struct_ty(def, vec![]);
    let ctor_def = ctx.register(DefKind::Function, "initialize", vec![]);
    let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
    let mut ctor = FunctionBuilder::new("initialize", ctx.interner.void());
    ctor.set_def(ctor_def, vec![]);
    let this = ctor.new_param(view, Some("this".into()));
    ctor.assign(
        Place::Field {
            base: this,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Int(42))),
    );
    ctor.terminate(Terminator::Return(None));
    let mut caller = FunctionBuilder::new("caller", ctx.interner.void());
    caller.set_def(caller_def, vec![]);
    let dest = caller.new_local(view, Some("value".into()));
    caller.push(Statement::Call {
        callee: crate::Callee {
            def: ctor_def,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(dest))],
    });
    caller.terminate(Terminator::Return(None));
    let mut mir = crate::Mir {
        functions: vec![ctor.finish(), caller.finish()],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    assert!(
        mir.functions[1]
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(s,
        Statement::Assign(Place::Field { base, field: 0 }, _) if *base == dest))
    );
}

#[test]
fn inlined_borrowed_value_parameter_does_not_drop_caller_fields() {
    let mut ctx = TypeCtx::new();
    let value_def = ctx.register(DefKind::Struct, "Payload", vec![]);
    ctx.defs.mark_value(value_def);
    ctx.interner.mark_value_def(value_def);
    let value = ctx.interner.struct_ty(value_def, vec![]);
    let callee_def = ctx.register(DefKind::Function, "read", vec![]);
    let mut b = FunctionBuilder::new("read", ctx.interner.int());
    b.set_def(callee_def, vec![]);
    b.new_param(value, Some("payload".into()));
    b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(1)))));
    let callee = b.finish();
    let mut b = FunctionBuilder::new("caller", ctx.interner.int());
    let input = b.new_local(value, Some("input".into()));
    let result = b.new_temp(ctx.interner.int());
    b.assign(
        Place::Local(result),
        Rvalue::Call {
            callee: crate::Callee {
                def: callee_def,
                args: vec![],
                ret: ctx.interner.int(),
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(input))],
        },
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
        result,
    )))));
    let mut mir = crate::Mir {
        functions: vec![callee, b.finish()],
        ..Default::default()
    };
    assert!(Inliner.run(&mut mir, &ctx.interner));
    let caller = mir.functions.iter().find(|f| f.name == "caller").unwrap();
    assert!(
        !caller
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(s, Statement::ValueDrop(_)))
    );
    let param = caller
        .locals
        .iter()
        .find(|d| d.name.as_deref() == Some("payload"))
        .unwrap();
    assert!(param.manual_drop);
    assert!(
        !param.is_ref,
        "borrowed by-value arguments keep their private copy"
    );
}
