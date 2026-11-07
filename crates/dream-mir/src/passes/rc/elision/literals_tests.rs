use super::*;
use crate::{Callee, build::FunctionBuilder};
use dream_types::{DefKind, TypeCtx};

fn rc_count(f: &MirFunction) -> usize {
    f.blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter(|s| {
            matches!(
                s,
                Statement::Retain(_)
                    | Statement::Release(_)
                    | Statement::ValueRetain(_)
                    | Statement::ValueDrop(_)
            )
        })
        .count()
}

fn hold(b: &mut FunctionBuilder, local: Local) {
    b.push(Statement::Retain(Operand::Copy(Place::Local(local))));
    b.push(Statement::Release(Operand::Copy(Place::Local(local))));
}

#[test]
fn copy_cycles_need_a_constant_seed_and_reject_any_unknown_input() {
    for unknown in [false, true] {
        let types = TypeInterner::new();
        let mut b = FunctionBuilder::new("cycle", types.void());
        let p = b.new_param(types.string(), None);
        let a = b.new_temp(types.string());
        let c = b.new_temp(types.string());
        b.assign(
            Place::Local(a),
            Rvalue::Use(Operand::Const(Const::Str("seed".into()))),
        );
        b.assign(Place::Local(c), Rvalue::Use(Operand::Copy(Place::Local(a))));
        b.assign(Place::Local(a), Rvalue::Move { src: c, cast: None });
        if unknown {
            b.assign(Place::Local(c), Rvalue::Use(Operand::Copy(Place::Local(p))));
        }
        hold(&mut b, a);
        hold(&mut b, c);
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        assert_eq!(
            elide_literal_rc(&mut f, &types, &LayoutTable::default()),
            !unknown
        );
        assert_eq!(rc_count(&f), if unknown { 4 } else { 0 });
    }
}

#[test]
fn selection_requires_both_values_to_be_immortal() {
    for unknown in [false, true] {
        let types = TypeInterner::new();
        let mut b = FunctionBuilder::new("select", types.void());
        let p = b.new_param(types.string(), None);
        let cond = b.new_param(types.bool(), None);
        let dest = b.new_temp(types.string());
        b.assign(
            Place::Local(dest),
            Rvalue::Select {
                cond: Operand::Copy(Place::Local(cond)),
                then_val: Operand::Const(Const::Str("yes".into())),
                else_val: if unknown {
                    Operand::Copy(Place::Local(p))
                } else {
                    Operand::Const(Const::Null)
                },
            },
        );
        hold(&mut b, dest);
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        assert_eq!(
            elide_literal_rc(&mut f, &types, &LayoutTable::default()),
            !unknown
        );
        assert_eq!(rc_count(&f), if unknown { 2 } else { 0 });
    }
}

fn view_type(types: &mut TypeCtx, layouts: &mut LayoutTable, name: &str, field: TypeId) -> TypeId {
    let def = types.register(DefKind::Struct, name, vec![]);
    types.interner.mark_value_def(def);
    let ty = types.interner.struct_ty(def, vec![]);
    let (size, align) = layouts
        .get(field)
        .map_or((4, 4), |layout| (layout.size, layout.align));
    layouts.insert(
        ty,
        dream_hir::TypeLayout {
            name: name.into(),
            fields: vec![dream_hir::FieldLayout {
                offset: 0,
                ty: field,
                name: "source".into(),
                is_weak: false,
                is_unowned: false,
            }],
            size,
            align,
            ..Default::default()
        },
    );
    ty
}

#[test]
fn nested_private_views_follow_field_aliases_without_changing_their_storage_mode() {
    let mut types = TypeCtx::new();
    let mut layouts = LayoutTable::default();
    let string = types.interner.string();
    let view = view_type(&mut types, &mut layouts, "View", string);
    let wrapper = view_type(&mut types, &mut layouts, "Wrapper", view);
    let mut b = FunctionBuilder::new("nested", types.interner.void());
    let parent = b.new_local(wrapper, Some("parent".into()));
    let alias = b.new_temp(view);
    let text = b.new_temp(string);
    b.assign(Place::Local(parent), Rvalue::Zero(wrapper));
    b.assign(
        Place::Local(alias),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: parent,
            field: 0,
        })),
    );
    b.assign(
        Place::Field {
            base: alias,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Str("literal".into()))),
    );
    b.assign(
        Place::Local(text),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: alias,
            field: 0,
        })),
    );
    hold(&mut b, text);
    b.push(Statement::ValueRetain(parent));
    b.push(Statement::ValueDrop(parent));
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    assert!(elide_literal_rc(&mut f, &types.interner, &layouts));
    assert_eq!(rc_count(&f), 0);
    assert!(f.locals[parent.0 as usize].borrows_refs);
    assert!(f.locals[parent.0 as usize].manual_drop);
    assert!(f.locals[alias.0 as usize].borrows_refs);
    assert!(
        !f.locals[alias.0 as usize].manual_drop,
        "an interior alias must stay an alias"
    );
    assert!(!elide_literal_rc(&mut f, &types.interner, &layouts));
}

#[test]
fn opaque_calls_unknown_writes_and_parameters_block_field_elision() {
    for mode in 0..4 {
        let mut types = TypeCtx::new();
        let mut layouts = LayoutTable::default();
        let string = types.interner.string();
        let view = view_type(&mut types, &mut layouts, "View", string);
        let mut b = FunctionBuilder::new("unknown", types.interner.void());
        let p = b.new_param(string, None);
        let owner = if mode == 2 {
            b.new_param(view, None)
        } else {
            b.new_local(view, Some("view".into()))
        };
        let alias = b.new_temp(view);
        let text = b.new_temp(string);
        if mode != 2 {
            b.assign(Place::Local(owner), Rvalue::Zero(view));
        }
        b.assign(
            Place::Field {
                base: owner,
                field: 0,
            },
            Rvalue::Use(Operand::Const(Const::Str("literal".into()))),
        );
        b.assign(
            Place::Local(alias),
            Rvalue::Use(Operand::Copy(Place::Local(owner))),
        );
        if mode == 0 {
            b.assign(
                Place::Field {
                    base: alias,
                    field: 0,
                },
                Rvalue::Use(Operand::Copy(Place::Local(p))),
            );
        } else if mode == 1 {
            b.push(Statement::Call {
                callee: Callee {
                    def: dream_types::DefId::root(900),
                    args: vec![],
                    ret: types.interner.void(),
                    take_params: vec![],
                    borrowed_result: false,
                },
                args: vec![Operand::Copy(Place::Local(alias))],
            });
        } else if mode == 3 {
            layouts.structs.get_mut(&view).unwrap().destructor =
                Some(dream_types::DefId::root(901));
        }
        b.assign(
            Place::Local(text),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: owner,
                field: 0,
            })),
        );
        hold(&mut b, text);
        b.push(Statement::ValueDrop(owner));
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        assert!(
            !elide_literal_rc(&mut f, &types.interner, &layouts),
            "mode {mode}"
        );
        assert_eq!(rc_count(&f), 3, "mode {mode}");
        assert!(!f.locals[owner.0 as usize].borrows_refs);
    }
}

#[test]
fn mutable_class_fields_are_never_literal_storage() {
    let mut types = TypeCtx::new();
    let def = types.register(DefKind::Struct, "Box", vec![]);
    let class = types.interner.struct_ty(def, vec![]);
    let mut layouts = LayoutTable::default();
    layouts.insert(
        class,
        dream_hir::TypeLayout::from_fields(
            &types.interner,
            "Box",
            [("text".into(), types.interner.string(), false, false)],
        ),
    );
    let mut b = FunctionBuilder::new("class", types.interner.void());
    let owner = b.new_param(class, None);
    let text = b.new_temp(types.interner.string());
    b.assign(
        Place::Field {
            base: owner,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Str("literal".into()))),
    );
    b.assign(
        Place::Local(text),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: owner,
            field: 0,
        })),
    );
    hold(&mut b, text);
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    assert!(!elide_literal_rc(&mut f, &types.interner, &layouts));
    assert_eq!(rc_count(&f), 2);
}

#[test]
fn a_dynamic_sibling_keeps_value_cleanup_but_not_literal_field_rc() {
    let mut types = TypeCtx::new();
    let def = types.register(DefKind::Struct, "Mixed", vec![]);
    types.interner.mark_value_def(def);
    let mixed = types.interner.struct_ty(def, vec![]);
    let string = types.interner.string();
    let mut layouts = LayoutTable::default();
    layouts.insert(
        mixed,
        dream_hir::TypeLayout::from_fields(
            &types.interner,
            "Mixed",
            [
                ("literal".into(), string, false, false),
                ("dynamic".into(), string, false, false),
            ],
        ),
    );
    let mut b = FunctionBuilder::new("siblings", types.interner.void());
    let param = b.new_param(string, None);
    let owner = b.new_local(mixed, Some("mixed".into()));
    let literal = b.new_temp(string);
    let dynamic = b.new_temp(string);
    b.assign(Place::Local(owner), Rvalue::Zero(mixed));
    b.assign(
        Place::Field {
            base: owner,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Str("static".into()))),
    );
    b.assign(
        Place::Field {
            base: owner,
            field: 1,
        },
        Rvalue::Use(Operand::Copy(Place::Local(param))),
    );
    b.assign(
        Place::Local(literal),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: owner,
            field: 0,
        })),
    );
    b.assign(
        Place::Local(dynamic),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: owner,
            field: 1,
        })),
    );
    hold(&mut b, literal);
    hold(&mut b, dynamic);
    b.push(Statement::ValueRetain(owner));
    b.push(Statement::ValueDrop(owner));
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    assert!(elide_literal_rc(&mut f, &types.interner, &layouts));
    assert_eq!(rc_count(&f), 4);
    assert!(!f.locals[owner.0 as usize].borrows_refs);
}
