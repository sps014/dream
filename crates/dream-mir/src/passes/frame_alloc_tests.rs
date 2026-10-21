use super::frame_alloc;
use crate::{Mir, NewCtor, Operand, Place, Rvalue, Statement, Terminator, build::FunctionBuilder};
use dream_hir::TypeLayout;
use dream_types::{DefKind, TypeCtx};

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
