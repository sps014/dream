use super::pipeline::Inliner;
use crate::build::FunctionBuilder;
use crate::passes::ModulePass;
use crate::passes::{MirPass, RcInsertion};
use crate::Const;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use crate::Terminator;
use dream_types::{DefKind, TypeCtx};

#[test]
fn owning_returns_transfer_into_every_destination() {
    for borrowed in [false, true] {
        for destination in ["local", "field", "index", "global"] {
            let mut ctx = TypeCtx::new();
            let ty = ctx.interner.string();
            let array = ctx.interner.array(ty);
            let holder_def = ctx.register(DefKind::Struct, "Holder", vec![]);
            let holder = ctx.interner.struct_ty(holder_def, vec![]);
            let make_def = ctx.register(DefKind::Function, "make", vec![]);
            let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
            let mut make = FunctionBuilder::new("make", ty);
            make.set_def(make_def, vec![]);
            let value = if borrowed {
                make.new_param(ty, Some("input".into()))
            } else {
                let value = make.new_local(ty, None);
                make.assign(
                    Place::Local(value),
                    Rvalue::Concat(vec![
                        Operand::Const(Const::Str("fresh".into())),
                        Operand::Const(Const::Str("result".into())),
                    ]),
                );
                value
            };
            make.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
            let mut make = make.finish();
            RcInsertion.run(&mut make, &ctx.interner);

            let mut caller = FunctionBuilder::new("caller", ctx.interner.void());
            caller.set_def(caller_def, vec![]);
            let input = caller.new_param(ty, Some("input".into()));
            let slot = caller.new_local(ty, None);
            let container_ty = if destination == "field" {
                holder
            } else {
                array
            };
            let base = caller.new_param(container_ty, Some("container".into()));
            let dest = match destination {
                "local" => Place::Local(slot),
                "field" => Place::Field { base, field: 0 },
                "index" => Place::index(base, Operand::Const(Const::Int(0))),
                "global" => Place::Global(crate::Global(0)),
                _ => unreachable!(),
            };
            caller.assign(
                dest,
                Rvalue::Call {
                    callee: crate::Callee {
                        def: make_def,
                        args: vec![],
                        ret: ty,
                        take_params: vec![],
                    },
                    args: if borrowed {
                        vec![Operand::Copy(Place::Local(input))]
                    } else {
                        vec![]
                    },
                },
            );
            caller.terminate(Terminator::Return(None));
            let mut mir = crate::Mir {
                functions: vec![caller.finish(), make],
                globals: vec![crate::MirGlobal {
                    id: crate::Global(0),
                    ty,
                }],
                ..Default::default()
            };
            mir.layouts.insert(
                holder,
                dream_hir::TypeLayout::from_fields(
                    &ctx.interner,
                    "Holder",
                    [("value".into(), ty, false, false)],
                ),
            );
            assert!(Inliner.run(&mut mir, &ctx.interner));
            let caller = &mir.functions[0];
            let transfer = caller.blocks.iter().flat_map(|b| b.stmts.windows(2)).any(|pair| {
                matches!(
                    (&pair[0], &pair[1]),
                    (Statement::Assign(_, Rvalue::Move { src, cast: None }),
                     Statement::Assign(Place::Local(cleared), Rvalue::Use(Operand::Const(Const::Null))))
                        if src == cleared
                )
            });
            assert!(
                transfer,
                "{destination}, borrowed={borrowed}: {:?}",
                caller.blocks
            );
            if borrowed {
                assert!(
                    caller
                        .blocks
                        .iter()
                        .flat_map(|b| &b.stmts)
                        .any(|stmt| { matches!(stmt, Statement::Retain(_)) }),
                    "a borrowed return must acquire its ABI token before transferring it"
                );
            }
        }
    }
}

#[test]
fn non_owning_fields_keep_the_call_result_disposal_boundary() {
    for (weak, unowned) in [(true, false), (false, true)] {
        let mut ctx = TypeCtx::new();
        let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
        let node = ctx.interner.struct_ty(node_def, vec![]);
        let holder_def = ctx.register(DefKind::Struct, "Holder", vec![]);
        let holder = ctx.interner.struct_ty(holder_def, vec![]);
        let make_def = ctx.register(DefKind::Function, "make", vec![]);
        let caller_def = ctx.register(DefKind::Function, "caller", vec![]);
        let mut make = FunctionBuilder::new("make", node);
        make.set_def(make_def, vec![]);
        let value = make.new_local(node, None);
        make.assign(
            Place::Local(value),
            Rvalue::New {
                def: node_def,
                ty: node,
                ctor: None,
                args: vec![],
            },
        );
        make.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
        let mut make = make.finish();
        RcInsertion.run(&mut make, &ctx.interner);
        let mut caller = FunctionBuilder::new("caller", ctx.interner.void());
        caller.set_def(caller_def, vec![]);
        let base = caller.new_param(holder, None);
        caller.assign(
            Place::Field { base, field: 0 },
            Rvalue::Call {
                callee: crate::Callee {
                    def: make_def,
                    args: vec![],
                    ret: node,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        caller.terminate(Terminator::Return(None));
        let mut mir = crate::Mir {
            functions: vec![caller.finish(), make],
            ..Default::default()
        };
        mir.layouts.insert(
            holder,
            dream_hir::TypeLayout::from_fields(
                &ctx.interner,
                "Holder",
                [("value".into(), node, weak, unowned)],
            ),
        );
        assert!(!Inliner.run(&mut mir, &ctx.interner));
        assert!(mir.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .any(|s| matches!(
                s,
                Statement::Assign(Place::Field { .. }, Rvalue::Call { .. })
            )));
    }
}
