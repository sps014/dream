use super::*;
use crate::build::FunctionBuilder;
use dream_hir::{FieldLayout, TypeLayout};
use dream_types::{DefKind, TypeCtx};

fn fixture(take: bool, mutation: bool, escape: bool) -> (Mir, TypeInterner) {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "View", vec![]);
    ctx.interner.mark_value_def(def);
    let view = ctx.interner.struct_ty(def, vec![]);
    let string = ctx.interner.string();
    let mut layouts = LayoutTable::default();
    layouts.insert(
        view,
        TypeLayout {
            name: "View".into(),
            fields: vec![FieldLayout {
                offset: 0,
                ty: string,
                name: "source".into(),
                is_weak: false,
                is_unowned: false,
            }],
            size: 4,
            align: 4,
            ..Default::default()
        },
    );
    let mut b = FunctionBuilder::new("read", if escape { view } else { ctx.interner.void() });
    let p = if take {
        b.new_take_param(view, Some("view".into()))
    } else {
        b.new_param(view, Some("view".into()))
    };
    let copy = b.new_local(view, Some("copy".into()));
    b.assign(
        Place::Local(copy),
        Rvalue::Use(Operand::Copy(Place::Local(p))),
    );
    b.push(Statement::ValueRetain(copy));
    if mutation {
        b.assign(
            Place::Field { base: p, field: 0 },
            Rvalue::Use(Operand::Const(Const::Str("new".into()))),
        );
    }
    b.push(Statement::ValueDrop(copy));
    b.terminate(Terminator::Return(
        escape.then_some(Operand::Copy(Place::Local(p))),
    ));
    (
        Mir {
            functions: vec![b.finish()],
            layouts,
            ..Default::default()
        },
        ctx.interner,
    )
}

#[test]
fn by_value_borrow_and_copies_drop_no_references() {
    let (mut mir, types) = fixture(false, false, false);
    assert!(run(&mut mir, &types));
    let f = &mir.functions[0];
    for d in &f.locals {
        assert!(d.borrows_refs && d.manual_drop);
        assert!(!d.is_ref, "by-value parameters must keep private storage");
    }
    assert!(
        !f.blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(s, Statement::ValueRetain(_) | Statement::ValueDrop(_)))
    );
    assert!(
        !run(&mut mir, &types),
        "a second run must report no changes"
    );
}

#[test]
fn owning_mutated_or_escaping_parameters_keep_arc() {
    for (take, mutation, escape) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let (mut mir, types) = fixture(take, mutation, escape);
        let f = &mir.functions[0];
        assert!(!signatures(&mir, &types)[&(f.def, f.instance.clone())][0]);
        assert!(!run(&mut mir, &types));
        assert!(mir.functions[0].locals.iter().all(|d| !d.borrows_refs));
        assert!(
            mir.functions[0].blocks[0]
                .stmts
                .iter()
                .any(|s| matches!(s, Statement::ValueRetain(_)))
        );
    }
}

fn owned_view(early_release: bool, opaque_call: bool, source_escape: bool) -> (Mir, TypeInterner) {
    let (mut mir, types) = fixture(false, false, false);
    let view_ty = mir.functions[0].local_ty(mir.functions[0].params[0]);
    let mut b = FunctionBuilder::new("owned_view", types.void());
    let owner = b.new_local(types.string(), Some("owner".into()));
    let view = b.new_local(view_ty, Some("view".into()));
    let read = b.new_local(types.string(), Some("read".into()));
    b.assign(
        Place::Local(owner),
        Rvalue::Concat(vec![
            Operand::Const(Const::Str("first".into())),
            Operand::Const(Const::Str("second".into())),
        ]),
    );
    b.assign(
        Place::Local(view),
        Rvalue::New {
            def: DefId::root(1),
            ty: view_ty,
            ctor: None,
            args: vec![],
            policy: crate::AllocPolicy::Tracked,
        },
    );
    b.assign(
        Place::Field {
            base: view,
            field: 0,
        },
        Rvalue::Use(Operand::Copy(Place::Local(owner))),
    );
    if early_release {
        b.push(Statement::Release(Operand::Copy(Place::Local(owner))));
    }
    if opaque_call || source_escape {
        b.push(Statement::Call {
            callee: Callee {
                def: DefId::root(99),
                args: vec![],
                ret: types.void(),
                take_params: vec![source_escape],
            },
            args: vec![Operand::Copy(Place::Local(owner))],
        });
    }
    b.assign(
        Place::Local(read),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: view,
            field: 0,
        })),
    );
    b.push(Statement::ValueDrop(view));
    if !early_release && !source_escape {
        b.push(Statement::Release(Operand::Copy(Place::Local(owner))));
    }
    b.terminate(Terminator::Return(None));
    mir.profile = dream_abi::profile::CompileProfile::Release;
    mir.functions = vec![b.finish()];
    (mir, types)
}

#[test]
fn local_string_owner_covers_its_views_without_extending_cleanup() {
    let (mut mir, types) = owned_view(false, false, false);
    assert!(run(&mut mir, &types));
    let f = &mir.functions[0];
    assert!(f.locals[1].borrows_refs);
    assert!(
        f.blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(s, Statement::Release(_)))
    );
}

#[test]
fn early_release_opaque_observers_and_transferred_owners_keep_view_arc() {
    for (early, opaque, escape) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let (mut mir, types) = owned_view(early, opaque, escape);
        run(&mut mir, &types);
        assert!(!mir.functions[0].locals[1].borrows_refs);
    }
}

#[test]
fn debug_keeps_owned_view_validation() {
    let (mut mir, types) = owned_view(false, false, false);
    mir.profile = dream_abi::profile::CompileProfile::Debug;
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn concatenation_that_can_alias_an_external_string_keeps_view_arc() {
    let (mut mir, types) = owned_view(false, false, false);
    let f = &mut mir.functions[0];
    let external = Local(f.locals.len() as u32);
    let mut input = f.locals[0].clone();
    input.name = Some("external".into());
    f.locals.push(input);
    f.params.push(external);
    f.blocks[0].stmts[0] = Statement::Assign(
        Place::Local(Local(0)),
        Rvalue::Concat(vec![
            Operand::Const(Const::Str("".into())),
            Operand::Copy(Place::Local(external)),
        ]),
    );
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn publication_before_view_creation_keeps_owned_view_arc() {
    let (mut mir, types) = owned_view(false, false, false);
    mir.functions[0].blocks[0].stmts.insert(
        1,
        Statement::Call {
            callee: Callee {
                def: DefId::root(99),
                args: vec![],
                ret: types.void(),
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(Local(0)))],
        },
    );
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn repeating_a_definition_cannot_replace_the_owner_of_a_live_view() {
    let (mut mir, types) = owned_view(false, false, false);
    let f = &mut mir.functions[0];
    let mut branch = f.locals[0].clone();
    branch.ty = types.bool();
    branch.name = None;
    f.locals.push(branch);
    f.params.push(Local(3));
    let mut stmts = std::mem::take(&mut f.blocks[0].stmts);
    let read = stmts.split_off(3);
    let initialization = stmts.split_off(1);
    f.blocks[0].stmts = stmts;
    f.blocks[0].terminator = Terminator::If {
        cond: Operand::Copy(Place::Local(Local(3))),
        then_blk: crate::BlockId(1),
        else_blk: crate::BlockId(2),
    };
    f.blocks.extend([
        crate::BasicBlock {
            stmts: initialization,
            terminator: Terminator::Goto(crate::BlockId(0)),
        },
        crate::BasicBlock {
            stmts: read,
            terminator: Terminator::Return(None),
        },
    ]);
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn release_on_one_predecessor_keeps_view_arc_at_the_join() {
    let (mut mir, types) = owned_view(false, false, false);
    let f = &mut mir.functions[0];
    let split = f.blocks[0]
        .stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(Place::Local(Local(2)), _)))
        .unwrap();
    let rest = f.blocks[0].stmts.split_off(split);
    f.blocks[0].terminator = Terminator::If {
        cond: Operand::Const(Const::Bool(true)),
        then_blk: crate::BlockId(1),
        else_blk: crate::BlockId(2),
    };
    f.blocks.extend([
        crate::BasicBlock {
            stmts: vec![Statement::Release(Operand::Copy(Place::Local(Local(0))))],
            terminator: Terminator::Goto(crate::BlockId(3)),
        },
        crate::BasicBlock {
            stmts: vec![],
            terminator: Terminator::Goto(crate::BlockId(3)),
        },
        crate::BasicBlock {
            stmts: rest,
            terminator: Terminator::Return(None),
        },
    ]);
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn named_copies_cannot_hide_opaque_forwarding() {
    let (mut mir, types) = fixture(false, false, false);
    let f = &mut mir.functions[0];
    f.blocks[0].stmts.push(Statement::Call {
        callee: Callee {
            def: DefId::root(99),
            args: vec![],
            ret: types.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(Local(1)))],
    });
    let f = &mir.functions[0];
    assert!(!signatures(&mir, &types)[&(f.def, f.instance.clone())][0]);
    assert!(!run(&mut mir, &types));
}
