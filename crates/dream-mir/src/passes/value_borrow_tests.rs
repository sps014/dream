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
                borrowed_result: false,
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
fn scalar_array_reallocation_cannot_keep_a_view_on_the_previous_block() {
    let (mut mir, mut types) = owned_view(false, false, false);
    let array = types.array(types.int());
    let f = &mut mir.functions[0];
    f.locals[0].ty = array;
    f.locals[2].ty = array;
    let view = f.locals[1].ty;
    let mut layout = mir.layouts.get(view).unwrap().clone();
    layout.fields[0].ty = array;
    mir.layouts.insert(view, layout);
    f.blocks[0].stmts[0] = Statement::Assign(
        Place::Local(Local(0)),
        Rvalue::ArrayLit {
            elem_ty: types.int(),
            elems: vec![Operand::Const(Const::Int(42))],
        },
    );
    let result = Local(f.locals.len() as u32);
    f.locals.push(f.locals[0].clone());
    f.blocks[0].stmts.insert(
        3,
        Statement::Assign(
            Place::Local(result),
            Rvalue::ArrayRealloc {
                elem_ty: types.int(),
                array: Operand::Copy(Place::Local(Local(0))),
                new_len: Operand::Const(Const::Int(32)),
            },
        ),
    );
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn owned_array_views_require_the_original_owner_on_every_path() {
    for early_release in [false, true] {
        let (mut mir, mut types) = owned_view(early_release, false, false);
        let array = types.array(types.int());
        let f = &mut mir.functions[0];
        f.locals[0].ty = array;
        f.locals[2].ty = array;
        let view = f.locals[1].ty;
        let mut layout = mir.layouts.get(view).expect("view layout").clone();
        layout.fields[0].ty = array;
        mir.layouts.insert(view, layout);
        f.blocks[0].stmts[0] = Statement::Assign(
            Place::Local(Local(0)),
            Rvalue::ArrayLit {
                elem_ty: types.int(),
                elems: vec![Operand::Const(Const::Int(42))],
            },
        );
        run(&mut mir, &types);
        assert_eq!(mir.functions[0].locals[1].borrows_refs, !early_release);
    }
}

#[test]
fn a_nonobserving_helper_can_write_an_unrelated_managed_field_around_a_view() {
    for observe in [false, true] {
        let (mut mir, mut types) = owned_view(false, false, false);
        let boxed = types.struct_ty(DefId::root(792), vec![]);
        mir.layouts.insert(
            boxed,
            dream_hir::TypeLayout::from_fields(
                &types,
                "Other",
                [("text".into(), types.string(), false, false)],
            ),
        );
        let mut helper = FunctionBuilder::new("write_other", types.void());
        helper.set_def(DefId::root(793), vec![]);
        let p = helper.new_param(boxed, None);
        helper.assign(
            Place::Field { base: p, field: 0 },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
        if observe {
            let count = helper.new_local(types.int(), None);
            helper.assign(
                Place::Local(count),
                Rvalue::ObservedLoad(Operand::Copy(Place::Local(p))),
            );
        }
        helper.terminate(Terminator::Return(None));
        mir.functions.push(helper.finish());
        let f = &mut mir.functions[0];
        let other = Local(f.locals.len() as u32);
        let mut decl = f.locals[0].clone();
        decl.ty = boxed;
        f.locals.push(decl);
        f.params.push(other);
        f.blocks[0].stmts.insert(
            3,
            Statement::Call {
                callee: Callee {
                    def: DefId::root(793),
                    args: vec![],
                    ret: types.void(),
                    take_params: vec![false],
                    borrowed_result: false,
                },
                args: vec![Operand::Copy(Place::Local(other))],
            },
        );
        run(&mut mir, &types);
        assert_eq!(mir.functions[0].locals[1].borrows_refs, !observe);
    }
}

#[test]
fn owned_strong_paths_cover_views_until_a_link_or_root_is_invalidated() {
    for mutation in 0..5 {
        let (mut mir, mut types) = owned_view(false, false, false);
        let root_ty = types.struct_ty(DefId::root(794), vec![]);
        let mut layout = dream_hir::TypeLayout::from_fields(
            &types,
            "Root",
            [("text".into(), types.string(), false, false)],
        );
        if mutation == 4 {
            layout.fields[0].is_weak = true;
        }
        mir.layouts.insert(root_ty, layout);
        let f = &mut mir.functions[0];
        let root = Local(f.locals.len() as u32);
        let mut decl = f.locals[0].clone();
        decl.ty = root_ty;
        f.locals.push(decl);
        f.locals[0].is_cursor = true;
        let source = Local(0);
        f.blocks[0].stmts.splice(
            0..1,
            [
                Statement::Assign(
                    Place::Local(root),
                    Rvalue::New {
                        def: DefId::root(794),
                        ty: root_ty,
                        ctor: None,
                        args: vec![],
                    },
                ),
                Statement::Assign(
                    Place::Field {
                        base: root,
                        field: 0,
                    },
                    Rvalue::Use(Operand::Const(Const::Str("kept".into()))),
                ),
                Statement::Assign(
                    Place::Local(source),
                    Rvalue::Use(Operand::Copy(Place::Field {
                        base: root,
                        field: 0,
                    })),
                ),
            ],
        );
        for stmt in &mut f.blocks[0].stmts {
            if matches!(stmt, Statement::Release(_)) {
                *stmt = Statement::Release(Operand::Copy(Place::Local(root)));
            }
        }
        let read = f.blocks[0]
            .stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(Place::Local(Local(2)), _)))
            .unwrap();
        if mutation == 1 || mutation == 3 {
            f.blocks[0].stmts.insert(
                read + usize::from(mutation == 3),
                Statement::Assign(
                    Place::Field {
                        base: root,
                        field: 0,
                    },
                    Rvalue::Use(Operand::Const(Const::Null)),
                ),
            );
        }
        if mutation == 2 {
            f.blocks[0]
                .stmts
                .insert(read, Statement::Release(Operand::Copy(Place::Local(root))));
        }
        run(&mut mir, &types);
        assert_eq!(
            mir.functions[0].locals[1].borrows_refs,
            matches!(mutation, 0 | 3),
            "mutation={mutation}"
        );
    }
}

#[test]
fn array_owner_paths_protect_only_the_selected_element() {
    for index in [0, 1] {
        let (mut mir, mut types) = owned_view(false, false, false);
        let array = types.array(types.string());
        let f = &mut mir.functions[0];
        let root = Local(f.locals.len() as u32);
        let mut decl = f.locals[0].clone();
        decl.ty = array;
        f.locals.push(decl);
        f.locals[0].is_cursor = true;
        f.blocks[0].stmts.splice(
            0..1,
            [
                Statement::Assign(
                    Place::Local(root),
                    Rvalue::ArrayLit {
                        elem_ty: types.string(),
                        elems: vec![
                            Operand::Const(Const::Str("kept".into())),
                            Operand::Const(Const::Str("other".into())),
                        ],
                    },
                ),
                Statement::Assign(
                    Place::Local(Local(0)),
                    Rvalue::Use(Operand::Copy(Place::index(
                        root,
                        Operand::Const(Const::Int(0)),
                    ))),
                ),
            ],
        );
        for stmt in &mut f.blocks[0].stmts {
            if matches!(stmt, Statement::Release(_)) {
                *stmt = Statement::Release(Operand::Copy(Place::Local(root)));
            }
        }
        let read = f.blocks[0]
            .stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(Place::Local(Local(2)), _)))
            .unwrap();
        f.blocks[0].stmts.insert(
            read,
            Statement::Assign(
                Place::index(root, Operand::Const(Const::Int(index))),
                Rvalue::Use(Operand::Const(Const::Null)),
            ),
        );
        run(&mut mir, &types);
        assert_eq!(mir.functions[0].locals[1].borrows_refs, index == 1);
    }
}

#[test]
fn empty_cleanup_after_a_kill_does_not_extend_the_view_lifetime() {
    let (mut mir, types) = owned_view(false, false, false);
    let f = &mut mir.functions[0];
    let at = f.blocks[0]
        .stmts
        .iter()
        .position(|s| matches!(s, Statement::ValueDrop(_)))
        .unwrap();
    f.blocks[0]
        .stmts
        .insert(at + 1, Statement::ValueKill(Local(1)));
    f.blocks[0].stmts.push(Statement::ValueDrop(Local(1)));
    let count = Local(f.locals.len() as u32);
    let mut decl = f.locals[0].clone();
    decl.ty = types.int();
    f.locals.push(decl);
    f.blocks[0].stmts.insert(
        0,
        Statement::Assign(
            Place::Local(count),
            Rvalue::ObservedLoad(Operand::Const(Const::Null)),
        ),
    );
    run(&mut mir, &types);
    assert!(mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn an_observation_after_the_final_read_still_protects_the_original_view_token() {
    let (mut mir, types) = owned_view(false, false, false);
    let f = &mut mir.functions[0];
    let at = f.blocks[0]
        .stmts
        .iter()
        .position(|s| matches!(s, Statement::ValueDrop(_)))
        .unwrap();
    let count = Local(f.locals.len() as u32);
    let mut decl = f.locals[0].clone();
    decl.ty = types.int();
    f.locals.push(decl);
    f.blocks[0].stmts.insert(
        at,
        Statement::Assign(
            Place::Local(count),
            Rvalue::ObservedLoad(Operand::Copy(Place::Local(Local(0)))),
        ),
    );
    run(&mut mir, &types);
    assert!(!mir.functions[0].locals[1].borrows_refs);
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
                borrowed_result: false,
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
            borrowed_result: false,
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

#[test]
fn value_parameter_forwarding_requires_every_callee_to_borrow() {
    for mutation in [false, true] {
        let (mut mir, types) = fixture(false, mutation, false);
        mir.profile = dream_abi::profile::CompileProfile::Release;
        mir.functions[0].def = DefId::root(450);
        let ty = mir.functions[0].local_ty(mir.functions[0].params[0]);
        let mut b = FunctionBuilder::new("relay", types.void());
        b.set_def(DefId::root(451), vec![]);
        let p = b.new_param(ty, None);
        b.push(Statement::Call {
            callee: Callee {
                def: DefId::root(450),
                args: vec![],
                ret: types.void(),
                take_params: vec![false],
                borrowed_result: false,
            },
            args: vec![Operand::Copy(Place::Local(p))],
        });
        b.terminate(Terminator::Return(None));
        mir.functions.push(b.finish());
        let sigs = signatures(&mir, &types);
        assert_eq!(sigs[&(DefId::root(451), vec![])][0], !mutation);
    }
}
