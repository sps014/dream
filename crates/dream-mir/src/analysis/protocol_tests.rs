use crate::build::FunctionBuilder;
use crate::{Const, Mir, Operand, Place, Rvalue, Terminator};
use dream_hir::{ObjectMethods, TypeLayout};
use dream_types::{DefKind, PrimTy, TypeCtx, TypeInterner};

#[test]
fn primitive_protocol_operations_do_not_require_dynamic_routers() {
    let mut types = TypeInterner::new();
    for primitive in [
        PrimTy::Int,
        PrimTy::UInt,
        PrimTy::Float,
        PrimTy::Double,
        PrimTy::Bool,
    ] {
        let mut b = FunctionBuilder::new("format", types.void());
        let value = b.new_param(types.prim(primitive), None);
        let text = b.new_temp(types.string());
        let hash = b.new_temp(types.int());
        b.assign(
            Place::Local(text),
            Rvalue::ToString(Operand::Copy(Place::Local(value))),
        );
        b.assign(
            Place::Local(hash),
            Rvalue::HashCode(Operand::Copy(Place::Local(value))),
        );
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Mir::default()
        };
        let reach = super::protocol::compute(&mir, &types, mir.functions.iter());
        assert!(
            !reach.dynamic,
            "{primitive:?} uses its own primitive operations"
        );
        assert!(reach.to_string.is_empty());
        assert!(reach.hash_code.is_empty());
    }
}

#[test]
fn unused_protocol_overrides_are_pruned_without_losing_destructors() {
    for operation in [0, 1, 2, 3] {
        let mut ctx = TypeCtx::new();
        let owner = ctx.register(DefKind::Struct, "Resource", vec![]);
        let ty = ctx.interner.struct_ty(owner, vec![]);
        let text_def = ctx.register(DefKind::Function, "render", vec![]);
        let hash_def = ctx.register(DefKind::Function, "hash", vec![]);
        let drop_def = ctx.register(DefKind::Function, "destroy", vec![]);
        let mut mir = Mir::default();
        let mut layout = TypeLayout::from_fields(&ctx.interner, "Resource", []);
        layout.destructor = Some(drop_def);
        mir.layouts.insert(ty, layout);
        mir.object_methods.insert(
            ty,
            ObjectMethods {
                to_string: Some(text_def),
                hash_code: Some(hash_def),
            },
        );
        for (def, name, ret) in [
            (text_def, "render", ctx.interner.string()),
            (hash_def, "hash", ctx.interner.int()),
            (drop_def, "destroy", ctx.interner.void()),
        ] {
            let mut b = FunctionBuilder::new(name, ret);
            b.set_def(def, vec![]);
            b.new_param(ty, None);
            let value = if def == text_def {
                Some(Operand::Const(Const::Str("ok".into())))
            } else if def == hash_def {
                Some(Operand::Const(Const::Int(7)))
            } else {
                None
            };
            b.terminate(Terminator::Return(value));
            mir.functions.push(b.finish());
        }
        let mut main = FunctionBuilder::new(crate::abi::ENTRY_FN, ctx.interner.void());
        main.set_def(ctx.register(DefKind::Function, "entry", vec![]), vec![]);
        let object = main.new_param(ty, None);
        if operation != 0 {
            let value = if operation == 3 {
                main.new_param(ctx.interner.object(), None)
            } else {
                object
            };
            let result = main.new_temp(if operation == 2 {
                ctx.interner.int()
            } else {
                ctx.interner.string()
            });
            let operand = Operand::Copy(Place::Local(value));
            main.assign(
                Place::Local(result),
                if operation == 2 {
                    Rvalue::HashCode(operand)
                } else {
                    Rvalue::ToString(operand)
                },
            );
        }
        main.terminate(Terminator::Return(None));
        mir.functions.push(main.finish());
        crate::prune_module(&mut mir, &ctx.interner);
        let retained = |def| mir.functions.iter().any(|f| f.def == def);
        assert!(retained(drop_def));
        assert_eq!(retained(text_def), operation == 1 || operation == 3);
        assert_eq!(retained(hash_def), operation == 2 || operation == 3);
    }
}

#[test]
fn generated_formatting_follows_fields_but_overrides_control_their_own_dependencies() {
    for overridden in [false, true] {
        let mut ctx = TypeCtx::new();
        let outer_def = ctx.register(DefKind::Struct, "Outer", vec![]);
        let inner_def = ctx.register(DefKind::Struct, "Inner", vec![]);
        let outer = ctx.interner.struct_ty(outer_def, vec![]);
        let inner = ctx.interner.struct_ty(inner_def, vec![]);
        let mut mir = Mir::default();
        mir.layouts.insert(
            outer,
            TypeLayout::from_fields(
                &ctx.interner,
                "Outer",
                [("child".into(), inner, false, false)],
            ),
        );
        mir.layouts.insert(
            inner,
            TypeLayout::from_fields(
                &ctx.interner,
                "Inner",
                [("erased".into(), ctx.interner.object(), false, false)],
            ),
        );
        if overridden {
            let render = ctx.register(DefKind::Function, "render", vec![]);
            mir.object_methods.insert(
                outer,
                ObjectMethods {
                    to_string: Some(render),
                    hash_code: None,
                },
            );
        }
        let mut b = FunctionBuilder::new("format", ctx.interner.void());
        let value = b.new_param(outer, None);
        let text = b.new_temp(ctx.interner.string());
        b.assign(
            Place::Local(text),
            Rvalue::ToString(Operand::Copy(Place::Local(value))),
        );
        b.terminate(Terminator::Return(None));
        mir.functions.push(b.finish());
        let reach = super::protocol::compute(&mir, &ctx.interner, mir.functions.iter());
        assert!(reach.to_string.contains(&outer));
        assert_eq!(reach.to_string.contains(&inner), !overridden);
        assert_eq!(reach.dynamic, !overridden);
    }
}
