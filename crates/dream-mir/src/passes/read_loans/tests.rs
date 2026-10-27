use super::*;
use crate::build::FunctionBuilder;
use dream_hir::{FieldLayout, TypeLayout};
use dream_types::{DefId, DefKind, TypeCtx};

fn fixture(take: bool, mutation: bool, escape: bool, observe: bool) -> (Mir, TypeInterner) {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Record", vec![]);
    ctx.interner.mark_value_def(def);
    let record = ctx.interner.struct_ty(def, vec![]);
    let string = ctx.interner.string();
    let array = ctx.interner.array(record);
    let mut layouts = LayoutTable::default();
    layouts.insert(
        record,
        TypeLayout {
            name: "Record".into(),
            fields: vec![
                FieldLayout {
                    offset: 0,
                    ty: string,
                    name: "text".into(),
                    is_weak: false,
                    is_unowned: false,
                },
                FieldLayout {
                    offset: 4,
                    ty: ctx.interner.int(),
                    name: "count".into(),
                    is_weak: false,
                    is_unowned: false,
                },
            ],
            size: 8,
            align: 4,
            ..Default::default()
        },
    );
    let mut b = FunctionBuilder::new("read", if escape { record } else { ctx.interner.void() });
    let p = if take {
        b.new_take_param(array, None)
    } else {
        b.new_param(array, None)
    };
    let copy = b.new_local(record, None);
    let read = b.new_local(string, None);
    b.assign(
        Place::Local(copy),
        Rvalue::Use(Operand::Copy(Place::index(
            p,
            Operand::Const(Const::Int(0)),
        ))),
    );
    b.push(Statement::ValueRetain(copy));
    if mutation {
        b.assign(
            Place::Field {
                base: copy,
                field: 1,
            },
            Rvalue::Use(Operand::Const(Const::Int(1))),
        );
    }
    let field = Operand::Copy(Place::Field {
        base: copy,
        field: 0,
    });
    b.assign(
        Place::Local(read),
        if observe {
            Rvalue::ObservedLoad(field)
        } else {
            Rvalue::Use(field)
        },
    );
    b.push(Statement::Retain(Operand::Copy(Place::Local(read))));
    b.push(Statement::Release(Operand::Copy(Place::Local(read))));
    b.push(Statement::ValueDrop(copy));
    b.terminate(Terminator::Return(
        escape.then_some(Operand::Copy(Place::Local(copy))),
    ));
    (
        Mir {
            profile: dream_abi::profile::CompileProfile::Release,
            functions: vec![b.finish()],
            layouts,
            ..Default::default()
        },
        ctx.interner,
    )
}

#[test]
fn borrowed_array_record_and_projection_share_a_loan() {
    let (mut mir, types) = fixture(false, false, false, false);
    assert!(run(&mut mir, &types));
    let f = &mir.functions[0];
    assert!(f.locals[1].borrows_refs && f.locals[1].manual_drop);
    assert!(f.locals[2].is_cursor);
    assert!(!f.blocks[0].stmts.iter().any(|s| matches!(
        s,
        Statement::ValueRetain(_)
            | Statement::ValueDrop(_)
            | Statement::Retain(_)
            | Statement::Release(_)
    )));
    assert!(!run(&mut mir, &types));
}

#[test]
fn owning_mutated_escaping_and_observed_records_keep_their_tokens() {
    for (take, mutation, escape, observe) in [
        (true, false, false, false),
        (false, true, false, false),
        (false, false, true, false),
        (false, false, false, true),
    ] {
        let (mut mir, types) = fixture(take, mutation, escape, observe);
        assert!(!run(&mut mir, &types));
        assert!(!mir.functions[0].locals[1].borrows_refs);
        assert!(
            mir.functions[0].blocks[0]
                .stmts
                .iter()
                .any(|s| matches!(s, Statement::ValueDrop(_)))
        );
    }
}

#[test]
fn private_array_mutation_does_not_disable_an_unrelated_record_loan() {
    let (mut mir, mut types) = fixture(false, false, false, false);
    let array = types.array(types.string());
    let f = &mut mir.functions[0];
    let scratch = Local(f.locals.len() as u32);
    let mut decl = f.locals[0].clone();
    decl.ty = array;
    f.locals.push(decl);
    f.blocks[0].stmts.splice(
        0..0,
        [
            Statement::Assign(
                Place::Local(scratch),
                Rvalue::ArrayLit {
                    elem_ty: types.string(),
                    elems: vec![Operand::Const(Const::Str("scratch".into()))],
                },
            ),
            Statement::Assign(
                Place::index(scratch, Operand::Const(Const::Int(0))),
                Rvalue::Use(Operand::Const(Const::Null)),
            ),
        ],
    );
    f.blocks[0]
        .stmts
        .push(Statement::Release(Operand::Copy(Place::Local(scratch))));
    assert!(run(&mut mir, &types));
    assert!(mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn a_helper_mutating_only_its_fresh_objects_preserves_record_loans() {
    let (mut mir, mut types) = fixture(false, false, false, false);
    let boxed_def = DefId::root(480);
    let boxed = types.struct_ty(boxed_def, vec![]);
    mir.layouts.insert(
        boxed,
        TypeLayout::from_fields(
            &types,
            "Scratch",
            [("text".into(), types.string(), false, false)],
        ),
    );
    let mut b = FunctionBuilder::new("private_work", types.void());
    b.set_def(DefId::root(481), vec![]);
    let object = b.new_local(boxed, None);
    b.assign(
        Place::Local(object),
        Rvalue::New {
            def: boxed_def,
            ty: boxed,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Field {
            base: object,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Str("scratch".into()))),
    );
    b.push(Statement::Release(Operand::Copy(Place::Local(object))));
    b.terminate(Terminator::Return(None));
    mir.functions.push(b.finish());
    mir.functions[0].blocks[0].stmts.insert(
        0,
        Statement::Call {
            callee: crate::Callee {
                def: DefId::root(481),
                args: vec![],
                ret: types.void(),
                take_params: vec![],
                borrowed_result: false,
            },
            args: vec![],
        },
    );
    assert!(run(&mut mir, &types));
    assert!(mir.functions[0].locals[1].borrows_refs);
}

#[test]
fn returned_alias_keeps_the_owning_retain_on_its_copy() {
    let (mut mir, types) = fixture(false, false, false, false);
    let f = &mut mir.functions[0];
    f.ret = types.string();
    let source = Local(2);
    let token = Local(f.locals.len() as u32);
    f.locals.push(f.locals[source.0 as usize].clone());
    let block = &mut f.blocks[0];
    block
        .stmts
        .retain(|s| !matches!(s, Statement::Retain(_) | Statement::Release(_)));
    block.stmts.extend([
        Statement::Assign(
            Place::Local(token),
            Rvalue::Use(Operand::Copy(Place::Local(source))),
        ),
        Statement::Retain(Operand::Copy(Place::Local(token))),
    ]);
    // Global propagation may return the source instead of the local carrying its ABI token.
    block.terminator = Terminator::Return(Some(Operand::Copy(Place::Local(source))));

    assert!(run(&mut mir, &types));
    let f = &mir.functions[0];
    assert!(f.locals[1].borrows_refs);
    assert!(!f.locals[token.0 as usize].is_cursor);
    assert!(
        f.blocks[0]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == token))
    );
}
