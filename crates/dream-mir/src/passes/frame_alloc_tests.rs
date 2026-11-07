use super::frame_alloc;
use crate::{
    Const, Mir, NewCtor, Operand, Place, Rvalue, Statement, Terminator, build::FunctionBuilder,
};
use dream_hir::TypeLayout;
use dream_types::{DefKind, TypeCtx, TypeId};

#[test]
fn frame_budget_includes_headers_and_alignment() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Wide", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut mir = Mir::default();
    mir.layouts.insert(
        ty,
        TypeLayout::from_fields(
            &ctx.interner,
            "Wide",
            [("field".into(), ctx.interner.int(), false, false)],
        ),
    );
    let mut b = FunctionBuilder::new("wide", ctx.interner.void());
    let mut objects = Vec::new();
    for _ in 0..40 {
        let object = b.new_local(ty, None);
        b.assign(
            Place::Local(object),
            Rvalue::New {
                def,
                ty,
                ctor: None::<NewCtor>,
                args: vec![],
            },
        );
        objects.push(object);
    }
    for object in objects {
        b.push(Statement::Release(Operand::Copy(Place::Local(object))));
    }
    b.terminate(Terminator::Return(None));
    mir.functions.push(b.finish());
    assert!(frame_alloc::run(&mut mir, &ctx.interner));
    assert_eq!(
        mir.frame_objects.len(),
        32,
        "forty 32-byte buffers exceed 1024 bytes"
    );
}

#[test]
fn frame_buffer_is_sized_by_the_constructed_class_not_the_declared_type() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Value", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut mir = Mir::default();
    mir.layouts.insert(
        ty,
        TypeLayout::from_fields(
            &ctx.interner,
            "Value",
            [("n".into(), ctx.interner.int(), false, false)],
        ),
    );
    let payload = mir.layouts.get(ty).unwrap().size;
    let mut b = FunctionBuilder::new("erased", ctx.interner.void());
    let erased = b.new_local(ctx.interner.object(), None);
    b.assign(
        Place::Local(erased),
        Rvalue::New {
            def,
            ty,
            ctor: None::<NewCtor>,
            args: vec![],
        },
    );
    b.push(Statement::Release(Operand::Copy(Place::Local(erased))));
    b.terminate(Terminator::Return(None));
    mir.functions.push(b.finish());
    assert!(frame_alloc::run(&mut mir, &ctx.interner));
    assert!(payload > 0);
    assert_eq!(
        mir.frame_objects.values().copied().collect::<Vec<_>>(),
        vec![payload]
    );
}

fn array_function(ctx: &mut TypeCtx, elem: TypeId, len: i64, escapes: bool) -> Mir {
    let arr_ty = ctx.interner.array(elem);
    let mut mir = Mir::default();
    let mut b = FunctionBuilder::new("arrays", ctx.interner.void());
    let arr = b.new_local(arr_ty, None);
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: elem,
            len: Operand::Const(Const::Int(len)),
            closure_env: false,
        },
    );
    if escapes {
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(arr)))));
    } else {
        b.push(Statement::Release(Operand::Copy(Place::Local(arr))));
        b.terminate(Terminator::Return(None));
    }
    mir.functions.push(b.finish());
    mir
}

#[test]
fn constant_length_scalar_array_is_built_in_the_frame() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let mut mir = array_function(&mut ctx, int, 64, false);
    assert!(frame_alloc::run(&mut mir, &ctx.interner));
    assert_eq!(
        mir.frame_objects.values().copied().collect::<Vec<_>>(),
        vec![8 + 64 * 4]
    );
    assert!(
        mir.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .all(|s| !matches!(s, Statement::Release(_))),
        "the array's release is its death and frees nothing"
    );
}

#[test]
fn array_reallocated_each_iteration_reuses_one_frame_buffer() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let arr_ty = ctx.interner.array(int);
    let mut mir = Mir::default();
    let mut b = FunctionBuilder::new("churn", ctx.interner.void());
    let go = b.new_param(ctx.interner.bool(), None);
    let held = b.new_local(arr_ty, None);
    let fresh = b.new_local(arr_ty, None);
    let tmp = b.new_local(arr_ty, None);
    let cond = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(Place::Local(held), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(go)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(fresh),
        Rvalue::ArrayNew {
            elem_ty: int,
            len: Operand::Const(Const::Int(8)),
            closure_env: false,
        },
    );
    b.assign(
        Place::Local(tmp),
        Rvalue::Move {
            src: fresh,
            cast: None,
        },
    );
    b.assign(
        Place::Local(fresh),
        Rvalue::Use(Operand::Const(Const::Null)),
    );
    b.push(Statement::Release(Operand::Copy(Place::Local(held))));
    b.assign(
        Place::Local(held),
        Rvalue::Use(Operand::Copy(Place::Local(tmp))),
    );
    b.assign(Place::Local(tmp), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::Goto(cond));
    b.switch_to(exit);
    b.push(Statement::Release(Operand::Copy(Place::Local(held))));
    b.terminate(Terminator::Return(None));
    mir.functions.push(b.finish());
    assert!(frame_alloc::run(&mut mir, &ctx.interner));
    assert_eq!(mir.frame_objects.len(), 1);
}

#[test]
fn arrays_that_escape_hold_references_or_are_large_stay_on_the_heap() {
    let mut ctx = TypeCtx::new();
    let int = ctx.interner.int();
    let string = ctx.interner.string();
    for (elem, len, escapes) in [(int, 8, true), (string, 8, false), (int, 1024, false)] {
        let mut mir = array_function(&mut ctx, elem, len, escapes);
        assert!(!frame_alloc::run(&mut mir, &ctx.interner));
        assert!(mir.frame_objects.is_empty());
    }
}

#[test]
fn cycle_capable_frame_objects_require_no_incoming_heap_edges() {
    for escapes in [false, true] {
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(def, vec![]);
        let mut mir = Mir::default();
        mir.layouts.insert(
            ty,
            TypeLayout::from_fields(&ctx.interner, "Node", [("next".into(), ty, false, false)]),
        );
        let mut b = FunctionBuilder::new("local", ctx.interner.void());
        let node = b.new_local(ty, None);
        b.assign(
            Place::Local(node),
            Rvalue::New {
                def,
                ty,
                ctor: None::<NewCtor>,
                args: vec![],
            },
        );
        if escapes {
            b.assign(
                Place::Field {
                    base: node,
                    field: 0,
                },
                Rvalue::Use(Operand::Copy(Place::Local(node))),
            );
        }
        b.push(Statement::Release(Operand::Copy(Place::Local(node))));
        b.terminate(Terminator::Return(None));
        mir.functions.push(b.finish());
        assert_eq!(frame_alloc::run(&mut mir, &ctx.interner), !escapes);
        assert_eq!(mir.frame_objects.is_empty(), escapes);
    }
}
