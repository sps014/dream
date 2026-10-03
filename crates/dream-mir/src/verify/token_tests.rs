use super::token_flow::check;
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, DefKind, TypeCtx, TypeId};

fn node(ctx: &mut TypeCtx) -> TypeId {
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    ctx.interner.struct_ty(def, vec![])
}

fn copy(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn fresh(ty: TypeId) -> Rvalue {
    Rvalue::Call {
        callee: Callee {
            def: DefId::root(100),
            args: vec![],
            ret: ty,
            take_params: vec![],
        },
        args: vec![],
    }
}

fn null(f: &mut FunctionBuilder, l: Local) {
    f.assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)));
}

#[test]
fn owning_call_results_require_exactly_one_drop() {
    for drops in 0..=2 {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("result", ctx.interner.void());
        let x = f.new_local(ty, None);
        f.assign(Place::Local(x), fresh(ty));
        for _ in 0..drops {
            f.push(Statement::Release(copy(x)));
        }
        f.terminate(Terminator::Return(None));
        let found = check(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), drops == 1, "{found:?}");
    }
}

#[test]
fn an_ignored_owning_call_result_must_be_materialized_by_lowering() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("discard", ctx.interner.void());
    let Rvalue::Call { callee, args } = fresh(ty) else {
        unreachable!()
    };
    f.push(Statement::Call { callee, args });
    f.terminate(Terminator::Return(None));
    assert!(check(&f.finish(), &ctx.interner)
        .iter()
        .any(|v| v.msg.contains("explicit destination")));
}

#[test]
fn taken_and_borrowed_parameters_have_distinct_obligations() {
    for take in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("parameter", ctx.interner.void());
        let x = if take {
            f.new_take_param(ty, None)
        } else {
            f.new_param(ty, None)
        };
        f.push(Statement::Release(copy(x)));
        f.terminate(Terminator::Return(None));
        assert_eq!(check(&f.finish(), &ctx.interner).is_empty(), take);
    }
}

#[test]
fn copies_need_a_token_but_moves_forward_the_existing_one() {
    for moved in [false, true] {
        for retained in [false, true] {
            let mut ctx = TypeCtx::new();
            let ty = node(&mut ctx);
            let mut f = FunctionBuilder::new("transfer", ctx.interner.void());
            let x = f.new_local(ty, None);
            let y = f.new_local(ty, None);
            f.assign(Place::Local(x), fresh(ty));
            f.assign(Place::Local(y), Rvalue::Use(copy(x)));
            if moved {
                null(&mut f, x);
            }
            if retained {
                f.push(Statement::Retain(copy(y)));
            }
            if !moved {
                f.push(Statement::Release(copy(x)));
            }
            f.push(Statement::Release(copy(y)));
            f.terminate(Terminator::Return(None));
            let found = check(&f.finish(), &ctx.interner);
            assert_eq!(found.is_empty(), moved != retained, "{found:?}");
        }
    }
}

#[test]
fn duplicate_taken_arguments_each_consume_one_token() {
    for retained in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("duplicate_take", ctx.interner.void());
        let x = f.new_local(ty, None);
        f.assign(Place::Local(x), fresh(ty));
        if retained {
            f.push(Statement::Retain(copy(x)));
        }
        f.push(Statement::Call {
            callee: Callee {
                def: DefId::root(101),
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![true, true],
            },
            args: vec![copy(x), copy(x)],
        });
        null(&mut f, x);
        f.terminate(Terminator::Return(None));
        let found = check(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), retained, "{found:?}");
    }
}

#[test]
fn indirect_calls_borrow_arguments_and_return_owned_results() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let sig = ctx.interner.func(vec![ty], ty);
    let mut f = FunctionBuilder::new("indirect", ty);
    let target = f.new_param(sig, None);
    let x = f.new_take_param(ty, None);
    let y = f.new_local(ty, None);
    f.assign(
        Place::Local(y),
        Rvalue::IndirectCall {
            target: copy(target),
            args: vec![copy(x)],
            sig,
        },
    );
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Return(Some(copy(y))));
    assert!(check(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn container_moves_consume_one_token_without_an_extra_drop() {
    for extra_drop in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("store", ctx.interner.void());
        let parent = f.new_param(ty, None);
        let x = f.new_local(ty, None);
        f.assign(Place::Local(x), fresh(ty));
        f.assign(
            Place::Field {
                base: parent,
                field: 0,
            },
            Rvalue::Move { src: x, cast: None },
        );
        if extra_drop {
            f.push(Statement::Release(copy(x)));
        }
        f.terminate(Terminator::Return(None));
        // The move zeros the source; releasing that null slot is harmless.
        assert!(check(&f.finish(), &ctx.interner).is_empty());
    }
}

#[test]
fn a_container_move_cannot_zero_untransferred_credits() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("extra_store_credit", ctx.interner.void());
    let parent = f.new_param(ty, None);
    let x = f.new_local(ty, None);
    f.assign(Place::Local(x), fresh(ty));
    f.push(Statement::Retain(copy(x)));
    f.assign(
        Place::Field {
            base: parent,
            field: 0,
        },
        Rvalue::Move { src: x, cast: None },
    );
    f.terminate(Terminator::Return(None));
    assert!(check(&f.finish(), &ctx.interner)
        .iter()
        .any(|v| v.msg.contains("overwritten")));
}

#[test]
fn every_incoming_path_must_balance_before_exit() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("diamond", ctx.interner.void());
    let cond = f.new_param(ctx.interner.bool(), None);
    let x = f.new_local(ty, None);
    let left = f.new_block();
    let right = f.new_block();
    let join = f.new_block();
    f.assign(Place::Local(x), fresh(ty));
    f.terminate(Terminator::If {
        cond: copy(cond),
        then_blk: left,
        else_blk: right,
    });
    f.switch_to(left);
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Goto(join));
    f.switch_to(right);
    f.terminate(Terminator::Goto(join));
    f.switch_to(join);
    f.terminate(Terminator::Return(None));
    assert!(check(&f.finish(), &ctx.interner)
        .iter()
        .any(|v| v.block == join.0 as usize && v.msg.contains("function exit")));
}

#[test]
fn loops_converge_and_reject_accumulating_tokens() {
    for balanced in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("loop", ctx.interner.void());
        let x = f.new_take_param(ty, None);
        let body = f.new_block();
        f.terminate(Terminator::Goto(body));
        f.switch_to(body);
        f.push(Statement::Retain(copy(x)));
        if balanced {
            f.push(Statement::Release(copy(x)));
        }
        f.terminate(Terminator::Goto(body));
        let found = check(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), balanced, "{found:?}");
    }
}

#[test]
fn returns_forward_only_one_token_and_overwrites_cannot_erase_tokens() {
    for overwrite in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("return", ty);
        let x = f.new_local(ty, None);
        f.assign(Place::Local(x), fresh(ty));
        if overwrite {
            f.assign(Place::Local(x), fresh(ty));
        } else {
            f.push(Statement::Retain(copy(x)));
        }
        f.terminate(Terminator::Return(Some(copy(x))));
        assert!(!check(&f.finish(), &ctx.interner).is_empty());
    }
}

#[test]
fn immortal_literals_have_no_drop_obligation() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("immortal", ctx.interner.string());
    let x = f.new_local(ctx.interner.string(), None);
    f.assign(
        Place::Local(x),
        Rvalue::Use(Operand::Const(Const::Str("literal".into()))),
    );
    f.terminate(Terminator::Return(Some(copy(x))));
    assert!(check(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn suspended_frames_preserve_one_cancellation_token_and_transfer_the_result() {
    for extra in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("poll", ty);
        let future = f.new_param(ty, None);
        let saved = f.new_local(ty, None);
        let result = f.new_local(ty, None);
        let resume = f.new_block();
        f.assign(Place::Local(saved), fresh(ty));
        if extra {
            f.push(Statement::Retain(copy(saved)));
        }
        f.terminate(Terminator::Await {
            future: copy(future),
            dest: Some(result),
            resume,
        });
        f.switch_to(resume);
        f.push(Statement::Release(copy(saved)));
        if extra {
            f.push(Statement::Release(copy(saved)));
        }
        f.terminate(Terminator::AsyncComplete(Some(copy(result))));
        let found = check(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), !extra, "{found:?}");
        if extra {
            assert!(found.iter().any(|v| v.msg.contains("cancellation token")));
        }
    }
}

#[test]
fn awaited_results_cannot_overwrite_a_live_owned_slot() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("await_overwrite", ty);
    let future = f.new_param(ty, None);
    let dest = f.new_local(ty, None);
    let resume = f.new_block();
    f.assign(Place::Local(dest), fresh(ty));
    f.terminate(Terminator::Await {
        future: copy(future),
        dest: Some(dest),
        resume,
    });
    f.switch_to(resume);
    f.terminate(Terminator::AsyncComplete(Some(copy(dest))));
    assert!(check(&f.finish(), &ctx.interner)
        .iter()
        .any(|v| v.msg.contains("overwritten")));
}

#[test]
fn released_await_destinations_must_be_cleared_before_cancellation_can_observe_them() {
    use crate::passes::{GlobalProp, MirPass, Sccp};
    for cleared in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("await_clear", ty);
        let future = f.new_param(ty, None);
        let dest = f.new_local(ty, None);
        let resume = f.new_block();
        f.assign(Place::Local(dest), fresh(ty));
        f.push(Statement::Release(copy(dest)));
        if cleared {
            null(&mut f, dest);
        }
        f.terminate(Terminator::Await {
            future: copy(future),
            dest: Some(dest),
            resume,
        });
        f.switch_to(resume);
        f.terminate(Terminator::AsyncComplete(Some(copy(dest))));
        let mut f = f.finish();
        let found = check(&f, &ctx.interner);
        assert_eq!(found.is_empty(), cleared, "{found:?}");
        Sccp.run(&mut f, &ctx.interner);
        GlobalProp.run(&mut f, &ctx.interner);
        assert!(
            matches!(f.blocks[resume.0 as usize].terminator, Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(l)))) if l == dest)
        );
    }
}
