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
