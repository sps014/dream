use super::*;
use crate::build::FunctionBuilder;
use crate::Callee;
use dream_types::{DefId, DefKind, TypeCtx};
use indexmap::IndexSet;

fn point_ty(ctx: &mut TypeCtx) -> dream_types::TypeId {
    let vs_def = ctx.register(DefKind::Struct, "Point", vec![]);
    ctx.defs.mark_value(vs_def);
    ctx.interner.mark_value_def(vs_def);
    ctx.interner.struct_ty(vs_def, vec![])
}

#[test]
fn last_use_temporary_value_call_kills_arg() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let make = ctx.register(DefKind::Function, "make", vec![]);
    let take = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let temp = b.new_temp(point);
    b.assign(
        Place::Local(temp),
        Rvalue::Call {
            callee: Callee {
                def: make,
                args: vec![],
                ret: point,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(temp))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(func.blocks[0]
        .stmts
        .iter()
        .any(|stmt| matches!(stmt, Statement::ValueKill(l) if *l == temp)));
    assert!(!func.blocks[0]
        .stmts
        .iter()
        .any(|stmt| matches!(stmt, Statement::ValueRetain(l) if *l == temp)));
}

#[test]
fn last_use_constructed_value_temporary_kills_arg() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let point_def = match ctx.interner.kind(point) {
        dream_types::TyKind::Struct(def, _) => *def,
        _other => panic!("{}", "point is a struct: {other:?}"),
    };
    let take = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let temp = b.new_temp(point);
    b.assign(
        Place::Local(temp),
        Rvalue::New {
            def: point_def,
            ty: point,
            ctor: None,
            args: vec![],
        },
    );
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(temp))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        func.blocks[0]
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Statement::ValueKill(l) if *l == temp)),
        "the sink callee owns the constructed value; frame teardown must not drop it again: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn last_use_value_assign_kills_source() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let s = b.new_local(point, Some("s".into()));
    let t = b.new_local(point, Some("t".into()));
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let kills: Vec<u32> = func.blocks[0]
        .stmts
        .iter()
        .filter_map(|st| match st {
            Statement::ValueKill(l) => Some(l.0),
            _ => None,
        })
        .collect();
    assert_eq!(kills, vec![s.0], "last-use dest=src should ValueKill src");
    assert!(func.locals[s.0 as usize].manual_drop);
    let retains = func.blocks[0]
        .stmts
        .iter()
        .filter(|st| matches!(st, Statement::ValueRetain(_)))
        .count();
    assert_eq!(retains, 0);
}

#[test]
fn still_live_value_assign_retains_dest() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let s = b.new_local(point, Some("s".into()));
    let t = b.new_local(point, Some("t".into()));
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.assign(Place::Local(s), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let has_retain_t = func.blocks[0]
        .stmts
        .iter()
        .any(|st| matches!(st, Statement::ValueRetain(l) if l.0 == t.0));
    assert!(has_retain_t, "still-live dest=src should ValueRetain dest");
}

#[test]
fn value_drop_waits_for_payload_borrower() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let string = ctx.interner.string();
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let s = b.new_local(point, Some("s".into()));
    let c = b.new_local(string, Some("c".into()));
    let n = b.new_temp(ctx.interner.int());
    b.assign(
        Place::Local(c),
        Rvalue::Use(Operand::Copy(Place::Field { base: s, field: 0 })),
    );
    b.assign(
        Place::Local(n),
        Rvalue::StrLen(Operand::Copy(Place::Local(c))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    func.locals[c.0 as usize].is_cursor = true;
    RcInsertion.run(&mut func, &ctx.interner);
    let stmts = &func.blocks[0].stmts;
    let drop_at = stmts
        .iter()
        .position(|st| matches!(st, Statement::ValueDrop(l) if l.0 == s.0))
        .expect("owning value local is dropped");
    let read_at = stmts
        .iter()
        .position(|st| matches!(st, Statement::Assign(Place::Local(d), _) if d.0 == n.0))
        .unwrap();
    assert!(
        drop_at > read_at,
        "drop must follow the borrower's last read"
    );
}

#[test]
fn value_moved_on_one_path_is_dropped_on_the_other() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let take_def = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let flag = b.new_param(ctx.interner.bool(), Some("flag".into()));
    let s = b.new_local(point, Some("s".into()));
    let moved = b.new_block();
    let kept = b.new_block();
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(flag)),
        then_blk: moved,
        else_blk: kept,
    });
    b.switch_to(moved);
    b.push(Statement::Call {
        callee: Callee {
            def: take_def,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(s))],
    });
    b.terminate(Terminator::Return(None));
    b.switch_to(kept);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(func.locals[s.0 as usize].manual_drop);
    let drops_in = |blk: crate::BlockId| {
        func.blocks[blk.0 as usize]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::ValueDrop(l) if l.0 == s.0))
    };
    assert!(drops_in(kept), "the path that keeps `s` must drop it");
}

#[test]
fn last_use_value_call_kills_arg() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let take_def = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let s = b.new_local(point, Some("s".into()));
    b.push(Statement::Call {
        callee: Callee {
            def: take_def,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(s))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let has_kill = func.blocks[0]
        .stmts
        .iter()
        .any(|st| matches!(st, Statement::ValueKill(l) if l.0 == s.0));
    let has_retain = func.blocks[0]
        .stmts
        .iter()
        .any(|st| matches!(st, Statement::ValueRetain(l) if l.0 == s.0));
    assert!(has_kill, "last-use call arg should ValueKill");
    assert!(!has_retain, "last-use call arg should not ValueRetain");
}

#[test]
fn still_live_value_call_retains_arg() {
    let mut ctx = TypeCtx::new();
    let point = point_ty(&mut ctx);
    let take_def = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let s = b.new_local(point, Some("s".into()));
    b.push(Statement::Call {
        callee: Callee {
            def: take_def,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(s))],
    });
    b.assign(Place::Local(s), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let has_retain = func.blocks[0]
        .stmts
        .iter()
        .any(|st| matches!(st, Statement::ValueRetain(l) if l.0 == s.0));
    assert!(has_retain, "still-live call arg should ValueRetain");
}

#[test]
fn still_live_iface_arg_retains() {
    let mut ctx = TypeCtx::new();
    let str_ty = ctx.interner.string();
    let sig = ctx.interner.func(vec![str_ty, str_ty], ctx.interner.void());
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let recv = b.new_local(str_ty, Some("h".into()));
    let rec = b.new_local(str_ty, Some("r".into()));
    b.push(Statement::InterfaceCall {
        receiver: Operand::Copy(Place::Local(recv)),
        iface_id: 0,
        method_slot: 0,
        sig,
        args: vec![Operand::Copy(Place::Local(rec))],
    });
    b.assign(
        Place::Local(rec),
        Rvalue::Use(Operand::Copy(Place::Local(rec))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let has_retain = func.blocks[0]
        .stmts
        .iter()
        .any(|st| matches!(st, Statement::Retain(Operand::Copy(Place::Local(l))) if l.0 == rec.0));
    assert!(has_retain, "still-live interface arg should Retain");
}

fn class_ty(ctx: &mut TypeCtx) -> (dream_types::DefId, dream_types::TypeId) {
    let def = ctx.register(DefKind::Struct, "User", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    (def, ty)
}

fn count_rc(func: &MirFunction) -> (usize, usize) {
    let mut retains = 0;
    let mut releases = 0;
    for b in &func.blocks {
        for s in &b.stmts {
            match s {
                Statement::Retain(_) => retains += 1,
                Statement::Release(_) => releases += 1,
                _ => {}
            }
        }
    }
    (retains, releases)
}

#[test]
fn birth_borrow_falls_off_block_one_release_zero_retain() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(x))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let (retains, releases) = count_rc(&func);
    assert_eq!(retains, 0, "borrow should not retain");
    assert_eq!(releases, 1, "token dies once: {:?}", func.blocks[0].stmts);
}

#[test]
fn last_use_assign_forwards_token_without_retain() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let y = b.new_local(ty, Some("y".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let (retains, releases) = count_rc(&func);
    assert_eq!(retains, 0);
    assert_eq!(releases, 1, "only y dies: {:?}", func.blocks[0].stmts);
    let nulls = func.blocks[0]
        .stmts
        .iter()
        .filter(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Null)))
            )
        })
        .count();
    assert_eq!(
        nulls, 2,
        "x is consumed and leftover y is nulled: {:?}",
        func.blocks[0].stmts
    );
}

/// `unwrap_or` None: leftover of `fallback` must not last-ref the returned alias.
#[test]
fn unwrap_or_none_does_not_release_returned_fallback() {
    let mut ctx = TypeCtx::new();
    let (_def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("unwrap_or", ty);
    let fallback = b.new_take_param(ty, Some("fallback".into()));
    let dest = b.new_local(ty, Some("switch_result".into()));
    b.assign(
        Place::Local(dest),
        Rvalue::Use(Operand::Copy(Place::Local(fallback))),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(dest)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let stmts = &func.blocks[0].stmts;
    let rel_fb = stmts.iter().position(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

            if *l == fallback
        )
    });
    let retain_dest = stmts
        .iter()
        .position(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == dest));
    assert!(
        rel_fb.is_none() || retain_dest.is_some_and(|r| rel_fb.is_some_and(|f| r > f)),
        "leftover fallback last-ref before return of alias: {:?}",
        stmts
    );
}

#[test]
fn still_live_alias_retains() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let y = b.new_local(ty, Some("y".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
    let take = ctx.register(DefKind::Function, "take", vec![]);
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(y))],
    });
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let (retains, _) = count_rc(&func);
    assert_eq!(retains, 1, "copy while x lives must retain");
}

#[test]
fn unbalanced_if_releases_on_kept_arm() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let take = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(x))],
    });
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let (retains, releases) = count_rc(&func);
    assert_eq!(retains, 0, "linear take vs unused arm");
    assert_eq!(releases, 1, "else arm consumes leftover token");
    let else_rel = func.blocks[else_blk.0 as usize]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(_)));
    assert!(else_rel, "release is on the arm that still held the token");
}

#[test]
fn take_param_loop_header_does_not_share_retain() {
    let mut ctx = TypeCtx::new();
    let (_, ty) = class_ty(&mut ctx);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let p = b.new_take_param(ty, Some("p".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(p))],
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert_eq!(
        retains, 0,
        "loop-header share Retain of a take param leaks +1: {:?}",
        func.blocks
    );
    let (_, releases) = count_rc(&func);
    assert_eq!(releases, 1, "take param still drops at return");
}

#[test]
fn loop_carried_local_does_not_share_retain_on_entry() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(x))],
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert_eq!(
        retains, 0,
        "back-edge is not a second owner: {:?}",
        func.blocks
    );
}

#[test]
fn loop_live_local_does_not_move_into_sink() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let take = ctx.register(DefKind::Function, "take", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(x))],
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert!(
        retains >= 1,
        "back-edge keeps x live so take copies: {:?}",
        func.blocks
    );
    let body_nulls = func.blocks[body.0 as usize]
        .stmts
        .iter()
        .filter(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Null)))
            )
        })
        .count();
    assert_eq!(body_nulls, 0, "must not move x inside the loop");
}

#[test]
fn loop_rebind_string_releases_previous() {
    let mut ctx = TypeCtx::new();
    let make = ctx.register(DefKind::Function, "make", vec![]);
    let take = ctx.register(DefKind::Function, "take", vec![]);
    let str_ty = ctx.interner.string();
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(str_ty, Some("x".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(
        Place::Local(x),
        Rvalue::Use(Operand::Const(Const::Str("seed".into()))),
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: make,
                args: vec![],
                ret: str_ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(x))],
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let body_rel = func.blocks[body.0 as usize]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l)))  if *l == x));
    assert!(
        body_rel,
        "overwrite of loop-carried string must drop the previous value: {:?}",
        func.blocks[body.0 as usize].stmts
    );
}

#[test]
fn rebind_evaluates_rhs_before_release() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mutate = ctx.register(DefKind::Function, "mutate_and_return", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: mutate,
                args: vec![],
                ret: ty,
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(x))],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let stmts = &func.blocks[0].stmts;
    let call_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(_, Rvalue::Call { .. })));
    let rel_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l)))  if *l == x));
    assert!(call_at.is_some() && rel_at.is_some(), "{:?}", stmts);
    assert!(
        call_at.unwrap() < rel_at.unwrap(),
        "Release(x) must follow tmp = mutate_and_return(x): {:?}",
        stmts
    );
}

#[test]
fn unread_local_survives_until_rebind() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let tmp = b.new_local(ctx.interner.int(), Some("tmp".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(tmp),
        Rvalue::Binary(
            crate::BinOp::Add,
            Operand::Const(Const::Int(1)),
            Operand::Const(Const::Int(2)),
        ),
    );
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let stmts = &func.blocks[0].stmts;
    let first_new = stmts
        .iter()
        .position(
            |s| matches!(s, Statement::Assign(Place::Local(l), Rvalue::New { .. }) if *l == x),
        )
        .unwrap();
    let add_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(_, Rvalue::Binary(..))))
        .unwrap();
    let early = stmts.iter().enumerate().any(|(idx, st)| {
        idx > first_new
            && idx < add_at
            && matches!(
                st,
                Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == x
            )
    });
    assert!(
        !early,
        "destroying x after the first New UAFs a later rebind's occupant: {:?}",
        stmts
    );
}

#[test]
fn ends_last_use_string_released_after_borrow_call() {
    let i = dream_types::TypeInterner::new();
    let peek = dream_types::DefId::root(0);
    let mut b = FunctionBuilder::new("f", i.void());
    let s = b.new_local(i.string(), Some("s".into()));
    let tmp = b.new_local(i.int(), Some("tmp".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(Const::Str("x".into()))),
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: i.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(s))],
    });
    b.assign(
        Place::Local(tmp),
        Rvalue::Binary(
            crate::BinOp::Add,
            Operand::Const(Const::Int(1)),
            Operand::Const(Const::Int(2)),
        ),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let stmts = &func.blocks[0].stmts;
    let call_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Call { .. }))
        .unwrap();
    let rel_at = stmts
        .iter()
        .position(|st| matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s));
    assert!(
        rel_at.is_some() && rel_at.unwrap() > call_at,
        "Ends borrow call last-use must Release after the call: {:?}",
        stmts
    );
}

#[test]
fn held_call_does_not_release_before_later_read() {
    let i = dream_types::TypeInterner::new();
    let peek = DefId::root(7);
    let mut b = FunctionBuilder::new("f", i.void());
    let s = b.new_local(i.string(), Some("s".into()));
    let n = b.new_local(i.int(), Some("n".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(Const::Str("x".into()))),
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: i.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(s))],
    });
    b.assign(
        Place::Local(n),
        Rvalue::StrLen(Operand::Copy(Place::Local(s))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    let mut holds = IndexSet::new();
    holds.insert(peek);
    RcInsertion::run_with_layouts(
        &mut func,
        &i,
        &dream_hir::LayoutTable::default(),
        &holds,
        &ModRefTable::default(),
    );
    let stmts = &func.blocks[0].stmts;
    let call_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Call { .. }))
        .unwrap();
    let len_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(_, Rvalue::StrLen(_))))
        .unwrap();
    let mid = stmts.iter().enumerate().any(|(idx, st)| {
        idx > call_at
            && idx < len_at
            && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s)
    });
    assert!(
        !mid,
        "Held call must not Release before a later read: {:?}",
        stmts
    );
    let after = stmts.iter().enumerate().any(|(idx, st)| {
        idx > len_at && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s)
    });
    assert!(after, "string still released after last read: {:?}", stmts);
}

#[test]
fn await_call_borrow_arg_not_released_before_await() {
    let i = dream_types::TypeInterner::new();
    let read = DefId::root(11);
    let mut b = FunctionBuilder::new("f", i.void());
    b.set_async(true);
    let path = b.new_local(i.string(), Some("path".into()));
    let fut = b.new_local(i.string(), Some("fut".into()));
    let resume = b.new_block();
    b.assign(
        Place::Local(path),
        Rvalue::Use(Operand::Const(Const::Str("p".into()))),
    );
    b.assign(
        Place::Local(fut),
        Rvalue::Call {
            callee: Callee {
                def: read,
                args: vec![],
                ret: i.string(),
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(path))],
        },
    );
    b.terminate(Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        dest: None,
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::AsyncComplete(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let await_rel = func.blocks[0].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == path
        )
    });
    assert!(
        !await_rel,
        "borrow arg of an awaited call must stay alive until resume: {:?}",
        func.blocks[0].stmts
    );
    let resume_rel = func.blocks[resume.0 as usize].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == path
        )
    });
    assert!(
        resume_rel,
        "path must Release after await resumes: {:?}",
        func.blocks[resume.0 as usize].stmts
    );
}

#[test]
fn loop_rebind_array_releases_previous() {
    let mut ctx = TypeCtx::new();
    let ch = ctx.interner.char();
    let arr_ty = ctx.interner.array(ch);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let wire = b.new_local(arr_ty, Some("wire".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(
        Place::Local(wire),
        Rvalue::ArrayNew {
            elem_ty: ch,
            len: Operand::Const(Const::Int(0)),
            closure_env: false,
        },
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(wire),
        Rvalue::ArrayNew {
            elem_ty: ch,
            len: Operand::Const(Const::Int(4)),
            closure_env: false,
        },
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let body_rel = func.blocks[body.0 as usize]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l)))  if *l == wire));
    assert!(
        body_rel,
        "loop-carried array rebind must drop the previous block: {:?}",
        func.blocks[body.0 as usize].stmts
    );
}

#[test]
fn loop_field_store_of_take_param_releases_at_return() {
    let mut ctx = TypeCtx::new();
    let (_def, ty) = class_ty(&mut ctx);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("insert", ctx.interner.void());
    let this = b.new_param(ty, Some("this".into()));
    let key = b.new_take_param(ty, Some("key".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Field {
            base: this,
            field: 0,
        },
        Rvalue::Use(Operand::Copy(Place::Local(key))),
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(key))],
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let exit_rel = func.blocks[exit.0 as usize]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l)))  if *l == key));
    assert!(
        exit_rel,
        "take-param retained into a field in a loop must leftover-Release: {:?}",
        func.blocks[exit.0 as usize].stmts
    );
}

#[test]
fn union_last_use_released_at_return() {
    let mut ctx = TypeCtx::new();
    let udef = ctx.register(DefKind::Union, "Opt", vec![]);
    let uty = ctx.interner.union_ty(udef, vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let a = b.new_local(uty, Some("a".into()));
    b.assign(
        Place::Local(a),
        Rvalue::UnionNew {
            def: udef,
            ty: uty,
            variant: 0,
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let rel = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l)))  if *l == a));
    assert!(
        rel,
        "owned union must Release before return: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn async_class_released_before_await() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    b.set_async(true);
    let x = b.new_local(ty, Some("x".into()));
    let fut = b.new_local(ty, Some("fut".into()));
    let resume = b.new_block();
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(fut),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        dest: None,
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::AsyncComplete(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let released_x = func.blocks[0].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == x
        )
    });
    assert!(
        released_x,
        "class last-used before await must Release in the Await block: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn async_complete_releases_sequential_hidden_borrow_locals() {
    let i = dream_types::TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    b.set_async(true);
    let s1 = b.new_local(i.string(), Some("s1".into()));
    let s2 = b.new_local(i.string(), Some("s2".into()));
    b.assign(
        Place::Local(s1),
        Rvalue::Use(Operand::Const(Const::Str("a".into()))),
    );
    b.assign(
        Place::Local(s2),
        Rvalue::Use(Operand::Const(Const::Str("b".into()))),
    );
    b.terminate(Terminator::AsyncComplete(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let complete = func
        .blocks
        .iter()
        .find(|bl| matches!(bl.terminator, Terminator::AsyncComplete(_)));
    let stmts = &complete.expect("AsyncComplete").stmts;
    let rel_s1 = stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == s1
        )
    });
    let rel_s2 = stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == s2
        )
    });
    assert!(
        rel_s1 && rel_s2,
        "both string locals must Release at AsyncComplete: {:?}",
        stmts
    );
}

#[test]
fn rebind_after_union_move_releases_dest_at_return() {
    // `a = None; a = b; b = None` — Release of the old `a` must not suppress the exit
    // drop of the value moved into `a`.
    let mut ctx = TypeCtx::new();
    let udef = ctx.register(DefKind::Union, "Opt", vec![]);
    let uty = ctx.interner.union_ty(udef, vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let a = b.new_local(uty, Some("a".into()));
    let bb = b.new_local(uty, Some("b".into()));
    b.assign(Place::Local(a), Rvalue::Use(Operand::Const(Const::Null)));
    b.assign(
        Place::Local(bb),
        Rvalue::UnionNew {
            def: udef,
            ty: uty,
            variant: 0,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(a),
        Rvalue::Use(Operand::Copy(Place::Local(bb))),
    );
    b.assign(Place::Local(bb), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let rel_a = func.blocks[0].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == a
        )
    });
    let last_rel_a = func.blocks[0].stmts.iter().rposition(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == a
        )
    });
    let move_a = func.blocks[0].stmts.iter().rposition(|s| {
        matches!(
            s,
            Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(src))))
                if *d == a && *src == bb
        )
    });
    assert!(rel_a, "dest must be released: {:?}", func.blocks[0].stmts);
    assert!(
        last_rel_a.unwrap() > move_a.unwrap(),
        "exit drop of a must follow a = b: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn async_await_rebind_releases_previous_dest() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    b.set_async(true);
    let s = b.new_local(ctx.interner.string(), Some("s".into()));
    let f1 = b.new_local(ty, Some("f1".into()));
    let f2 = b.new_local(ty, Some("f2".into()));
    let r1 = b.new_block();
    let r2 = b.new_block();
    b.assign(
        Place::Local(f1),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(f2),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Await {
        future: Operand::Copy(Place::Local(f1)),
        dest: Some(s),
        resume: r1,
    });
    b.switch_to(r1);
    b.terminate(Terminator::Await {
        future: Operand::Copy(Place::Local(f2)),
        dest: Some(s),
        resume: r2,
    });
    b.switch_to(r2);
    b.terminate(Terminator::AsyncComplete(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let r1_stmts = &func.blocks[r1.0 as usize].stmts;
    let released_s = r1_stmts.iter().any(|st| {
        matches!(
            st,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == s
        )
    });
    assert!(
        released_s,
        "re-await into the same dest must Release the previous value: {:?}",
        r1_stmts
    );
}

#[test]
fn loop_await_releases_future_on_resume() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    b.set_async(true);
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let fut = b.new_local(ty, Some("fut".into()));
    let header = b.new_block();
    let body = b.new_block();
    let resume = b.new_block();
    let exit = b.new_block();
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(fut),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        dest: None,
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::AsyncComplete(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let resume_rel = func.blocks[resume.0 as usize].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))

                if *l == fut
        )
    });
    assert!(
        resume_rel,
        "await future must Release on resume even if the next iteration drop_previous would keep it live: {:?}",
        func.blocks[resume.0 as usize].stmts
    );
}

#[test]
fn unique_new_uses_release_unique() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let has_release = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(_)));
    let has_retain = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Retain(_)));
    assert!(
        has_release,
        "unique leftover must still Release: {:?}",
        func.blocks[0].stmts
    );
    assert!(!has_retain);
}

#[test]
fn last_use_field_store_nulls_without_retain() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let obj = b.new_local(ty, Some("obj".into()));
    let x = b.new_local(ty, Some("x".into()));
    b.assign(
        Place::Local(obj),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Field {
            base: obj,
            field: 0,
        },
        Rvalue::Use(Operand::Copy(Place::Local(x))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let (retains, _) = count_rc(&func);
    assert_eq!(
        retains, 0,
        "last-use field store is a move: {:?}",
        func.blocks[0].stmts
    );
    let null_x = func.blocks[0].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                if *l == x
        )
    });
    assert!(
        null_x,
        "source nulled after container move: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn last_use_string_index_store_nulls_without_retain() {
    let mut ctx = TypeCtx::new();
    let str_ty = ctx.interner.string();
    let arr_ty = ctx.interner.array(str_ty);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let arr = b.new_local(arr_ty, Some("arr".into()));
    let s = b.new_local(str_ty, Some("s".into()));
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: str_ty,
            len: Operand::Const(Const::Int(1)),
            closure_env: false,
        },
    );
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(Const::Str("x".into()))),
    );
    b.assign(
        Place::index(arr, Operand::Const(Const::Int(0))),
        Rvalue::Use(Operand::Copy(Place::Local(s))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let null_s = func.blocks[0].stmts.iter().any(|st| {
        matches!(
            st,
            Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                if *l == s
        )
    });
    assert!(
        null_s,
        "source nulled after string[] store: {:?}",
        func.blocks[0].stmts
    );
    let store_at = func.blocks[0]
        .stmts
        .iter()
        .position(|st| matches!(st, Statement::Assign(Place::Index { .. }, _)))
        .expect("index store");
    let retain_after_store = func.blocks[0].stmts[store_at + 1..]
        .iter()
        .any(|st| matches!(st, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == s));
    assert!(
        !retain_after_store,
        "move into string[] must not Retain after the store: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn loop_string_index_store_is_per_iter_move() {
    let mut ctx = TypeCtx::new();
    let str_ty = ctx.interner.string();
    let arr_ty = ctx.interner.array(str_ty);
    let make = ctx.register(DefKind::Function, "make", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let arr = b.new_local(arr_ty, Some("arr".into()));
    let s = b.new_local(str_ty, Some("s".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: str_ty,
            len: Operand::Const(Const::Int(1)),
            closure_env: false,
        },
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(s),
        Rvalue::Call {
            callee: Callee {
                def: make,
                args: vec![],
                ret: str_ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    b.assign(
        Place::index(arr, Operand::Const(Const::Int(0))),
        Rvalue::Use(Operand::Copy(Place::Local(s))),
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let body_stmts = &func.blocks[body.0 as usize].stmts;
    let null_s = body_stmts.iter().any(|st| {
        matches!(
            st,
            Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                if *l == s
        )
    });
    let retain_s = body_stmts
        .iter()
        .any(|st| matches!(st, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == s));
    assert!(
        null_s && !retain_s,
        "loop-local string stored into array must move each iter: {:?}",
        body_stmts
    );
}

#[test]
fn still_live_copy_is_shared_not_unique_destroy() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let y = b.new_local(ty, Some("y".into()));
    b.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
    b.assign(Place::Local(x), Rvalue::Use(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let (retains, _) = count_rc(&func);
    assert!(
        retains >= 1,
        "still-live alias retains: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn string_never_release_unique() {
    let i = dream_types::TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let s = b.new_local(i.string(), Some("s".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(Const::Str("x".into()))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &i));
    let uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|st| matches!(st, Statement::ForceFree(_)));
    assert!(
        !uniq,
        "strings stay on ordinary release: {:?}",
        func.blocks[0].stmts
    );
}
