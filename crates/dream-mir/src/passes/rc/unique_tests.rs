//! Unique/Shared lattice edge cases for [`super::RcInsertion`].

use super::RcInsertion;
use crate::build::FunctionBuilder;
use crate::passes::MirPass;
use crate::{Callee, Const, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefKind, TypeCtx};

fn class_ty(ctx: &mut TypeCtx) -> (dream_types::DefId, dream_types::TypeId) {
    let def = ctx.register(DefKind::Struct, "User", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    (def, ty)
}

fn new_obj(def: dream_types::DefId, ty: dream_types::TypeId) -> Rvalue {
    Rvalue::New {
        def,
        ty,
        ctor: None,
        args: vec![],
    }
}

#[test]
fn diamond_both_arms_unique_no_join_retain() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.assign(Place::Local(x), new_obj(def, ty));
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert_eq!(retains, 0, "Unique∧Unique stays Unique: {:?}", func.blocks);
}

#[test]
fn diamond_unique_vs_shared_retains_on_unique_arm() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let y = b.new_local(ty, Some("y".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.assign(Place::Local(x), new_obj(def, ty));
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk,
        else_blk,
    });
    let take = ctx.register(DefKind::Function, "take", vec![]);
    b.switch_to(then_blk);
    b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(y))],
    });
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let else_retain = func.blocks[else_blk.0 as usize]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == x));
    assert!(
        else_retain,
        "Unique arm must Retain before Shared join: {:?}",
        func.blocks
    );
}

#[test]
fn phi_assign_unique_vs_shared_does_not_retain_moved_dest() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let d = b.new_local(ty, Some("d".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.assign(Place::Local(x), new_obj(def, ty));
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.assign(Place::Local(d), new_obj(def, ty));
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(d)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let then_retain_d = func.blocks[then_blk.0 as usize]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == d));
    assert!(
        !then_retain_d,
        "unique phi assign must not share-Retain dest: {:?}",
        func.blocks
    );
}

#[test]
fn loop_copy_inside_body_is_shared() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let y = b.new_local(ty, Some("y".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(Place::Local(x), new_obj(def, ty));
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    let take = ctx.register(DefKind::Function, "take", vec![]);
    b.switch_to(body);
    b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(y))],
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert!(
        retains >= 1,
        "loop-carried copy is Shared: {:?}",
        func.blocks
    );
}

#[test]
fn same_stmt_field_store_of_self_is_copy() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let n = b.new_local(ty, Some("n".into()));
    b.assign(Place::Local(n), new_obj(def, ty));
    b.assign(
        Place::Field { base: n, field: 0 },
        Rvalue::Use(Operand::Copy(Place::Local(n))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let rel = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l))) if *l == n));
    let uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(
        rel && !uniq,
        "n.next = n copies then ordinary-releases the local: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn take_param_field_store_then_use_does_not_null() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let obj = b.new_local(ty, Some("obj".into()));
    let p = b.new_take_param(ty, Some("p".into()));
    b.assign(Place::Local(obj), new_obj(def, ty));
    b.assign(
        Place::Field {
            base: obj,
            field: 0,
        },
        Rvalue::Use(Operand::Copy(Place::Local(p))),
    );
    b.assign(Place::Local(p), Rvalue::Use(Operand::Copy(Place::Local(p))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let stmts = &func.blocks[0].stmts;
    let store = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(Place::Field { base, .. }, _) if *base == obj))
        .expect("field store");
    assert!(
        !matches!(
            stmts.get(store + 1),
            Some(Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null))))
                if *l == p
        ),
        "still-live take after field store is not a move: {:?}",
        stmts
    );
}

#[test]
fn self_ref_field_call_is_not_container_move() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let step = ctx.register(DefKind::Function, "step", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let n = b.new_local(ty, Some("n".into()));
    b.assign(Place::Local(n), new_obj(def, ty));
    b.assign(
        Place::Field { base: n, field: 0 },
        Rvalue::Call {
            callee: Callee {
                def: step,
                args: vec![],
                ret: ty,
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Field { base: n, field: 0 })],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(
        uniq,
        "n itself can still unique-destroy; Call rvalue is not a local move: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn await_future_resume_release_is_not_unique() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    b.set_async(true);
    let fut = b.new_local(ty, Some("fut".into()));
    let resume = b.new_block();
    b.assign(Place::Local(fut), new_obj(def, ty));
    b.terminate(Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        dest: Some(fut),
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::AsyncComplete(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let await_uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(
        !await_uniq,
        "awaited handle must not unique-destroy in the Await block: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn shared_class_never_release_unique() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    ctx.interner.mark_shared_def(def);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    b.assign(Place::Local(x), new_obj(def, ty));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(
        !uniq,
        "@shared stays on ordinary Release: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn js_never_release_unique() {
    let i = dream_types::TypeInterner::new();
    let make = dream_types::DefId(0);
    let mut b = FunctionBuilder::new("f", i.void());
    let h = b.new_local(i.js(), Some("h".into()));
    b.assign(
        Place::Local(h),
        Rvalue::Call {
            callee: Callee {
                def: make,
                args: vec![],
                ret: i.js(),
                take_params: vec![],
            },
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(
        !uniq,
        "js stays on ordinary Release: {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn take_param_last_use_is_ordinary_release() {
    let mut ctx = TypeCtx::new();
    let (_def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let _p = b.new_take_param(ty, Some("p".into()));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(!uniq, "take params may be caller copies: {:?}", func.blocks);
}

#[test]
fn interface_typed_unique_still_release_unique() {
    let mut ctx = TypeCtx::new();
    let (def, class_ty) = class_ty(&mut ctx);
    let idef = ctx.register(DefKind::Interface, "I", vec![]);
    let ity = ctx.interner.interface_ty(idef, vec![]);
    let _ = class_ty;
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ity, Some("x".into()));
    b.assign(Place::Local(x), new_obj(def, ity));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_)));
    assert!(
        uniq,
        "interface unique last-use is ReleaseUnique (tag-dispatch destroy): {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn last_use_union_new_payload_is_not_unique_destroy() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let udef = ctx.register(DefKind::Union, "Opt", vec![]);
    let uty = ctx.interner.union_ty(udef, vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let r = b.new_local(uty, Some("r".into()));
    b.assign(Place::Local(x), new_obj(def, ty));
    b.assign(
        Place::Local(r),
        Rvalue::UnionNew {
            def: udef,
            ty: uty,
            variant: 0,
            args: vec![Operand::Copy(Place::Local(x))],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq_x = func.blocks[0].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) if *l == x
        )
    });
    assert!(
        !uniq_x,
        "UnionNew payload must not unique-destroy (emitter retains): {:?}",
        func.blocks[0].stmts
    );
}

#[test]
fn borrow_call_last_use_is_ordinary_release() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let iter = ctx.register(DefKind::Function, "iter", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    let y = b.new_local(ty, Some("y".into()));
    b.assign(Place::Local(x), new_obj(def, ty));
    b.assign(
        Place::Local(y),
        Rvalue::Call {
            callee: Callee {
                def: iter,
                args: vec![],
                ret: ty,
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(x))],
        },
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(y)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq_x = func.blocks.iter().flat_map(|bb| &bb.stmts).any(|s| {
        matches!(
            s,
            Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) if *l == x
        )
    });
    assert!(
        !uniq_x,
        "borrowed call args may be retained by the callee: {:?}",
        func.blocks
    );
}

#[test]
fn call_result_last_use_is_ordinary_release() {
    let mut ctx = TypeCtx::new();
    let (_def, ty) = class_ty(&mut ctx);
    let get = ctx.register(DefKind::Function, "get", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let x = b.new_local(ty, Some("x".into()));
    b.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: get,
                args: vec![],
                ret: ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq_x = func.blocks.iter().flat_map(|bb| &bb.stmts).any(|s| {
        matches!(
            s,
            Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) if *l == x
        )
    });
    assert!(
        !uniq_x,
        "call results may alias callee storage: {:?}",
        func.blocks
    );
}

#[test]
fn pointer_pun_to_int_is_not_unique_destroy() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = class_ty(&mut ctx);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let cell = b.new_local(ty, Some("cell".into()));
    let pun = b.new_local(ctx.interner.int(), Some("pun".into()));
    b.assign(Place::Local(cell), new_obj(def, ty));
    b.assign(
        Place::Local(pun),
        Rvalue::Use(Operand::Copy(Place::Local(cell))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let uniq = func.blocks.iter().flat_map(|bb| &bb.stmts).any(|s| {
        matches!(
            s,
            Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) if *l == cell
        )
    });
    assert!(
        !uniq,
        "int pun of a cell is a funcbox-style alias: {:?}",
        func.blocks
    );
}

#[test]
fn self_ref_union_new_does_not_share_at_loop_header() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Union, "List", vec![]);
    let ty = ctx.interner.union_ty(def, vec![]);
    let mut b = FunctionBuilder::new("f", ty);
    let list = b.new_local(ty, Some("list".into()));
    let c = b.new_local(ctx.interner.bool(), Some("c".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(
        Place::Local(list),
        Rvalue::UnionNew {
            def,
            ty,
            variant: 1,
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
    b.assign(
        Place::Local(list),
        Rvalue::UnionNew {
            def,
            ty,
            variant: 0,
            args: vec![Operand::Copy(Place::Local(list))],
        },
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(list)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| {
            matches!(
                s,
                Statement::Retain(Operand::Copy(Place::Local(l))) if *l == list
            )
        })
        .count();
    assert_eq!(
        retains, 0,
        "self-ref Cons stays Unique; extra header retain leaks the tail: {:?}",
        func.blocks
    );
}
