use super::*;
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefKind, TypeCtx};

#[test]
fn catches_release_then_use_in_a_successor() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("cross_block", ty);
    let x = f.new_local(ty, None);
    let next = f.new_block();
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Goto(next));
    f.switch_to(next);
    f.terminate(Terminator::Return(Some(copy(x))));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].block, next.0 as usize);
    assert!(found[0].msg.contains("only token was released"));
}

#[test]
fn catches_death_on_only_one_incoming_path() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("diamond", ty);
    let cond = f.new_param(ctx.interner.bool(), None);
    let x = f.new_local(ty, None);
    let left = f.new_block();
    let right = f.new_block();
    let join = f.new_block();
    f.assign(Place::Local(x), new_node(ty));
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
    f.terminate(Terminator::Return(Some(copy(x))));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].block, join.0 as usize);
}

#[test]
fn catches_double_release_through_a_backedge_but_allows_rebirth() {
    for rebirth in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("loop", ctx.interner.void());
        let x = f.new_local(ty, None);
        let body = f.new_block();
        f.assign(Place::Local(x), new_node(ty));
        f.terminate(Terminator::Goto(body));
        f.switch_to(body);
        if rebirth {
            f.assign(Place::Local(x), new_node(ty));
        }
        f.push(Statement::Release(copy(x)));
        f.terminate(Terminator::Goto(body));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), rebirth, "{found:?}");
    }
}

#[test]
fn a_redefinition_in_the_successor_clears_death() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("rebirth", ty);
    let x = f.new_local(ty, None);
    let next = f.new_block();
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Goto(next));
    f.switch_to(next);
    f.assign(Place::Local(x), new_node(ty));
    f.terminate(Terminator::Return(Some(copy(x))));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn checks_cfg_targets_before_running_dataflow() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("bad_cfg", ctx.interner.void());
    f.terminate(Terminator::Goto(crate::BlockId(99)));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1);
    assert!(found[0].msg.contains("successor bb99"));
}

#[test]
fn a_region_allocation_alias_cannot_cross_leave() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("stale_region_alias", ty);
    let x = f.new_local(ty, None);
    let alias = f.new_local(ty, None);
    let next = f.new_block();
    f.push(Statement::RegionEnter);
    f.assign(Place::Local(x), new_node(ty));
    f.assign(Place::Local(alias), Rvalue::Use(copy(x)));
    f.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Goto(next));
    f.switch_to(next);
    f.terminate(Terminator::Return(Some(copy(alias))));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].msg.contains("allocation region was left"));
}

#[test]
fn inner_rewind_does_not_kill_outer_allocations() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("nested_regions", ctx.interner.void());
    let x = f.new_local(ty, None);
    let alias = f.new_local(ty, None);
    f.push(Statement::RegionEnter);
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::RegionEnter);
    f.push(Statement::RegionLeave);
    f.assign(Place::Local(alias), Rvalue::Use(copy(x)));
    f.push(Statement::RegionLeave);
    f.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
    f.assign(
        Place::Local(alias),
        Rvalue::Use(Operand::Const(Const::Null)),
    );
    f.terminate(Terminator::Return(None));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn heap_allocations_before_a_region_stay_live() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("ordinary_heap", ty);
    let x = f.new_local(ty, None);
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::RegionEnter);
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(Some(copy(x))));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn niche_wrappers_preserve_the_payload_allocation_origin() {
    for allocated_inside in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let def = ctx.register(DefKind::Union, "Option", vec![]);
        let option = ctx.interner.union_ty(def, vec![ty]);
        ctx.interner.mark_niche_union(option);
        let mut f = FunctionBuilder::new("niche_origin", ty);
        let x = f.new_local(ty, None);
        let wrapped = f.new_local(option, None);
        let payload = f.new_local(ty, None);
        if !allocated_inside {
            f.assign(Place::Local(x), new_node(ty));
        }
        f.push(Statement::RegionEnter);
        if allocated_inside {
            f.assign(Place::Local(x), new_node(ty));
        }
        f.assign(
            Place::Local(wrapped),
            Rvalue::UnionNew {
                def,
                ty: option,
                variant: 0,
                args: vec![copy(x)],
            },
        );
        f.assign(
            Place::Local(payload),
            Rvalue::UnionField {
                base: copy(wrapped),
                ty: option,
                variant: 0,
                field: 0,
            },
        );
        f.push(Statement::RegionLeave);
        f.terminate(Terminator::Return(Some(copy(payload))));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), !allocated_inside, "{found:?}");
    }
}

#[test]
fn awaited_result_replaces_a_rewound_destination() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("await_redefinition", ty);
    let future = f.new_param(ty, None);
    let result = f.new_local(ty, None);
    let resume = f.new_block();
    f.push(Statement::RegionEnter);
    f.assign(Place::Local(result), new_node(ty));
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Await {
        future: copy(future),
        dest: Some(result),
        resume,
    });
    f.switch_to(resume);
    f.terminate(Terminator::Return(Some(copy(result))));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn region_leave_requires_an_enter() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("underflow", ctx.interner.void());
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(None));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1);
    assert!(found[0].msg.contains("without an active region"));
}

#[test]
fn balanced_regions_can_span_blocks_and_loops() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("balanced", ctx.interner.void());
    let cond = f.new_param(ctx.interner.bool(), None);
    let body = f.new_block();
    let exit = f.new_block();
    f.push(Statement::RegionEnter);
    f.terminate(Terminator::Goto(body));
    f.switch_to(body);
    f.push(Statement::RegionEnter);
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::If {
        cond: copy(cond),
        then_blk: body,
        else_blk: exit,
    });
    f.switch_to(exit);
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(None));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn unbalanced_regions_at_joins_and_backedges_are_rejected() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("region_loop", ctx.interner.void());
    let body = f.new_block();
    f.terminate(Terminator::Goto(body));
    f.switch_to(body);
    f.push(Statement::RegionEnter);
    f.terminate(Terminator::Goto(body));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].msg.contains("depth mismatch"));
}

#[test]
fn live_regions_cannot_escape_by_return_or_suspend() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("return_region", ctx.interner.void());
    f.push(Statement::RegionEnter);
    f.terminate(Terminator::Return(None));
    assert_eq!(verify_function(&f.finish(), &ctx.interner).len(), 1);

    let mut f = FunctionBuilder::new("suspend_region", ctx.interner.void());
    let resume = f.new_block();
    f.push(Statement::RegionEnter);
    f.terminate(Terminator::Await {
        future: Operand::Const(Const::Null),
        dest: None,
        resume,
    });
    f.switch_to(resume);
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(None));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(found.len(), 1);
    assert!(found[0].msg.contains("suspension"));
}

fn node(ctx: &mut TypeCtx) -> dream_types::TypeId {
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    ctx.interner.struct_ty(def, vec![])
}

fn new_node(ty: dream_types::TypeId) -> Rvalue {
    Rvalue::New {
        def: dream_types::DefId::root(0),
        ty,
        ctor: None,
        args: vec![],
    }
}

fn copy(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

#[test]
fn flags_return_after_last_release() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("f", ty);
    let x = f.new_local(ty, None);
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Return(Some(copy(x))));
    let v = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(v.len(), 1, "{:?}", v);
    assert!(v[0].msg.contains("only token was released"), "{:?}", v);
}

#[test]
fn flags_use_and_double_release_of_single_token_local() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("f", ctx.interner.void());
    let x = f.new_local(ty, None);
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::Release(copy(x)));
    f.push(Statement::Print {
        arg: copy(x),
        ty,
        newline: true,
    });
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Return(None));
    let v = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(
        v.len(),
        0,
        "print shares x, so it is not single-token: {:?}",
        v
    );

    let mut f = FunctionBuilder::new("g", ctx.interner.void());
    let x = f.new_local(ty, None);
    let n = f.new_local(ctx.interner.bool(), None);
    f.assign(Place::Local(x), new_node(ty));
    f.push(Statement::Release(copy(x)));
    f.assign(
        Place::Local(n),
        Rvalue::Binary(crate::BinOp::Eq, copy(x), Operand::Const(Const::Null)),
    );
    f.push(Statement::Release(copy(x)));
    f.terminate(Terminator::Return(None));
    let v = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(v.len(), 2, "{:?}", v);
    assert!(v[1].msg.contains("released twice"), "{:?}", v);
}

#[test]
fn retained_or_passed_aliases_are_not_single_token() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("f", ctx.interner.void());
    let x = f.new_local(ty, None);
    let y = f.new_local(ty, None);
    let z = f.new_local(ty, None);
    f.assign(Place::Local(x), new_node(ty));
    f.assign(Place::Local(y), Rvalue::Use(copy(x)));
    f.push(Statement::Retain(copy(y)));
    f.push(Statement::Release(copy(x)));
    f.assign(Place::Local(z), Rvalue::Use(copy(x)));
    f.push(Statement::Call {
        callee: Callee {
            def: dream_types::DefId::root(1),
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![],
        },
        args: vec![copy(z)],
    });
    f.terminate(Terminator::Return(None));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn flags_rc_op_on_scalar() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("f", ctx.interner.void());
    let i = f.new_local(ctx.interner.int(), None);
    f.assign(Place::Local(i), Rvalue::Use(Operand::Const(Const::Int(1))));
    f.push(Statement::Retain(copy(i)));
    f.terminate(Terminator::Return(None));
    let v = verify_function(&f.finish(), &ctx.interner);
    assert_eq!(v.len(), 1, "{:?}", v);
    assert!(v[0].msg.contains("non-RC"), "{:?}", v);
}
