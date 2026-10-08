use super::frame_alloc;
use crate::{Mir, NewCtor, Operand, Place, Rvalue, Statement, Terminator, build::FunctionBuilder};
use dream_hir::TypeLayout;
use dream_types::{DefKind, TypeCtx};

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
                policy: crate::AllocPolicy::Tracked,
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
