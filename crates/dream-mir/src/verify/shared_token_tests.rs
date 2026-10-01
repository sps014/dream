use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Local, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefKind, TypeCtx};

fn copy(local: Local) -> Operand {
    Operand::Copy(Place::Local(local))
}

fn fixture() -> (TypeCtx, FunctionBuilder, Local, Local) {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut f = FunctionBuilder::new("balance", ctx.interner.void());
    let x = f.new_local(ty, None);
    let alias = f.new_local(ty, None);
    f.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    f.assign(Place::Local(alias), Rvalue::Use(copy(x)));
    f.push(Statement::Retain(copy(alias)));
    (ctx, f, x, alias)
}

#[test]
fn retained_aliases_require_exactly_one_release_per_token() {
    for releases in 0..=3 {
        let (ctx, mut f, x, alias) = fixture();
        for index in 0..releases {
            f.push(Statement::Release(copy(if index == 0 { x } else { alias })));
        }
        f.terminate(Terminator::Return(None));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(
            found.is_empty(),
            releases == 2,
            "releases={releases}: {found:?}"
        );
    }
}

#[test]
fn retained_aliases_remain_readable_until_the_last_count_dies() {
    let (ctx, mut f, x, alias) = fixture();
    f.push(Statement::Release(copy(x)));
    let result = f.new_local(ctx.interner.bool(), None);
    f.assign(
        Place::Local(result),
        Rvalue::Binary(crate::BinOp::Eq, copy(alias), Operand::Const(Const::Null)),
    );
    f.push(Statement::Release(copy(alias)));
    f.terminate(Terminator::Return(None));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn scalar_inspection_cannot_hide_shared_token_errors() {
    for releases in [0, 2] {
        let (ctx, mut f, x, alias) = fixture();
        for _ in 0..releases {
            f.push(Statement::Release(copy(x)));
        }
        let result = f.new_local(ctx.interner.bool(), None);
        f.assign(
            Place::Local(result),
            Rvalue::Binary(crate::BinOp::Eq, copy(alias), Operand::Const(Const::Null)),
        );
        f.terminate(Terminator::Return(None));
        let found = verify_function(&f.finish(), &ctx.interner);
        let expected = if releases == 0 {
            "remain at function return"
        } else {
            "last RC token"
        };
        assert!(
            found.iter().any(|v| v.msg.contains(expected)),
            "{:?}",
            found
        );
    }
}

#[test]
fn each_incoming_path_must_balance_shared_tokens() {
    for balanced in [false, true] {
        let (ctx, mut f, x, alias) = fixture();
        let cond = f.new_param(ctx.interner.bool(), None);
        let left = f.new_block();
        let right = f.new_block();
        let join = f.new_block();
        f.terminate(Terminator::If {
            cond: copy(cond),
            then_blk: left,
            else_blk: right,
        });
        f.switch_to(left);
        f.push(Statement::Release(copy(x)));
        f.terminate(Terminator::Goto(join));
        f.switch_to(right);
        if balanced {
            f.push(Statement::Release(copy(alias)));
        }
        f.terminate(Terminator::Goto(join));
        f.switch_to(join);
        f.push(Statement::Release(copy(alias)));
        f.terminate(Terminator::Return(None));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), balanced, "{found:?}");
    }
}

#[test]
fn returns_forward_one_count_not_every_count_in_the_alias_family() {
    for release in [false, true] {
        let (ctx, mut f, x, alias) = fixture();
        if release {
            f.push(Statement::Release(copy(x)));
        }
        f.terminate(Terminator::Return(Some(copy(alias))));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), release, "{found:?}");
    }
}

#[test]
fn null_slots_do_not_consume_another_aliases_count() {
    let (ctx, mut f, x, alias) = fixture();
    f.push(Statement::Release(copy(x)));
    f.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
    f.push(Statement::Release(copy(x)));
    f.push(Statement::Release(copy(alias)));
    f.terminate(Terminator::Return(None));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn balanced_loops_converge_but_positive_balance_cycles_are_rejected() {
    for balanced in [false, true] {
        let (ctx, mut f, x, alias) = fixture();
        let body = f.new_block();
        f.terminate(Terminator::Goto(body));
        f.switch_to(body);
        f.push(Statement::Retain(copy(x)));
        if balanced {
            f.push(Statement::Release(copy(alias)));
        }
        f.terminate(Terminator::Goto(body));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), balanced, "{found:?}");
    }
}

#[test]
fn a_stale_alias_cannot_be_retained_in_a_successor() {
    let (ctx, mut f, x, alias) = fixture();
    let body = f.new_block();
    let ty = ctx
        .interner
        .iter_kinds()
        .find_map(|(ty, kind)| matches!(kind, dream_types::TyKind::Struct(..)).then_some(ty))
        .expect("node type");
    let old = f.new_local(ty, None);
    f.assign(Place::Local(old), Rvalue::Use(copy(alias)));
    f.push(Statement::Release(copy(x)));
    f.push(Statement::Release(copy(alias)));
    f.terminate(Terminator::Goto(body));
    f.switch_to(body);
    f.push(Statement::Retain(copy(old)));
    f.terminate(Terminator::Return(None));
    assert!(!verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn overwriting_a_slot_cannot_erase_an_owned_count() {
    let (ctx, mut f, x, alias) = fixture();
    f.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
    f.push(Statement::Release(copy(alias)));
    f.terminate(Terminator::Return(None));
    assert!(!verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn a_store_through_a_dead_alias_is_a_use_after_release() {
    let (ctx, mut f, x, alias) = fixture();
    f.push(Statement::Release(copy(x)));
    f.push(Statement::Release(copy(alias)));
    f.assign(
        Place::Field {
            base: alias,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Int(1))),
    );
    f.terminate(Terminator::Return(None));
    assert!(!verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn moves_transfer_a_binding_without_creating_a_token() {
    let (ctx, mut f, x, alias) = fixture();
    f.push(Statement::Release(copy(x)));
    f.assign(
        Place::Local(x),
        Rvalue::Move {
            src: alias,
            cast: None,
        },
    );
    f.push(Statement::Release(copy(alias)));
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Return(None));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn allocation_site_reentry_requires_the_old_generation_to_be_balanced() {
    for balanced in [false, true] {
        let (ctx, mut f, x, alias) = fixture();
        if balanced {
            f.push(Statement::Release(copy(x)));
            f.push(Statement::Release(copy(alias)));
        }
        f.terminate(Terminator::Goto(crate::BlockId(0)));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), balanced, "{found:?}");
    }
}

#[test]
fn scalar_tail_returns_cannot_leave_owned_tokens_behind() {
    for balanced in [false, true] {
        let (mut ctx, mut f, x, alias) = fixture();
        if balanced {
            f.push(Statement::Release(copy(x)));
            f.push(Statement::Release(copy(alias)));
        }
        let def = ctx.register(DefKind::Function, "tail", vec![]);
        f.terminate(Terminator::TailCall {
            callee: crate::Callee {
                def,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![],
            },
            args: vec![],
        });
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), balanced, "{found:?}");
    }
}
