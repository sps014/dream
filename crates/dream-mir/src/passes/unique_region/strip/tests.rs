use super::*;
use crate::build::FunctionBuilder;
use dream_types::{DefKind, TypeCtx};

#[test]
fn join_keeps_dangling_provenance_from_either_arm() {
    for (kill_left, kill_right) in [(false, true), (true, false), (true, true)] {
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(def, vec![]);
        let mut f = FunctionBuilder::new("diamond", ty);
        let cond = f.new_param(ctx.interner.bool(), None);
        let x = f.new_local(ty, None);
        let left = f.new_block();
        let right = f.new_block();
        let join = f.new_block();
        f.push(Statement::RegionLeave);
        f.terminate(Terminator::If {
            cond: copy(cond),
            then_blk: left,
            else_blk: right,
        });
        for (arm, kill) in [(left, kill_left), (right, kill_right)] {
            f.switch_to(arm);
            if kill {
                f.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
            }
            f.terminate(Terminator::Goto(join));
        }
        f.switch_to(join);
        f.terminate(Terminator::Return(Some(copy(x))));
        assert_eq!(
            rc_use_after_leave(&f.finish(), 0, 0, &BTreeSet::from([x.0])),
            !(kill_left && kill_right)
        );
    }
}

#[test]
fn backedge_cannot_hide_a_dangling_use_before_redefinition() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut f = FunctionBuilder::new("loop", ty);
    let x = f.new_local(ty, None);
    let alias = f.new_local(ty, None);
    f.assign(Place::Local(alias), Rvalue::Use(copy(x)));
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Goto(crate::BlockId(0)));
    assert!(rc_use_after_leave(
        &f.finish(),
        0,
        1,
        &BTreeSet::from([x.0])
    ));
}

#[test]
fn awaited_destination_redefinition_kills_old_provenance() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut f = FunctionBuilder::new("await", ty);
    let x = f.new_local(ty, None);
    let future = f.new_param(ty, None);
    let resume = f.new_block();
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Await {
        future: copy(future),
        dest: Some(x),
        resume,
    });
    f.switch_to(resume);
    f.terminate(Terminator::Return(Some(copy(x))));
    assert!(!rc_use_after_leave(
        &f.finish(),
        0,
        0,
        &BTreeSet::from([x.0])
    ));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "escaped inferred region in escaped")]
fn debug_pipeline_rejects_instead_of_silently_repairing() {
    let (ctx, mut mir) = escaped_module();
    strip_escaped_regions(&mut mir, &ctx.interner);
}

#[cfg(not(debug_assertions))]
#[test]
fn release_pipeline_removes_escaped_regions() {
    let (ctx, mut mir) = escaped_module();
    assert!(strip_escaped_regions(&mut mir, &ctx.interner));
    assert!(mir.functions[0].blocks.iter().all(|block| block
        .stmts
        .iter()
        .all(|stmt| !matches!(stmt, Statement::RegionEnter | Statement::RegionLeave))));
    assert!(crate::verify::verify_module(&mir, &ctx.interner).is_empty());
}

fn escaped_module() -> (TypeCtx, Mir) {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut f = FunctionBuilder::new("escaped", ty);
    let x = f.new_local(ty, None);
    f.push(Statement::RegionEnter);
    f.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(Some(copy(x))));
    let mir = Mir {
        functions: vec![f.finish()],
        ..Default::default()
    };
    (ctx, mir)
}

fn copy(local: Local) -> Operand {
    Operand::Copy(Place::Local(local))
}
