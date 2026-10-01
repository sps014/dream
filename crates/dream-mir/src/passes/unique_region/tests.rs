use super::*;
use crate::build::FunctionBuilder;
use crate::passes::ModulePass;
use dream_hir::{LayoutTable, TypeLayout};
use dream_types::{DefKind, TypeCtx};

#[test]
fn recursive_safety_resolves_entire_component_before_caching() {
    let mut ctx = TypeCtx::new();
    let a = ctx.register(DefKind::Function, "a", vec![]);
    let b = ctx.register(DefKind::Function, "b", vec![]);
    let caller = ctx.register(DefKind::Function, "caller", vec![]);
    let ty = ctx.interner.int();
    let build = |def, callee, escapes| {
        let mut builder = FunctionBuilder::new("recursive", ty);
        builder.set_def(def, vec![]);
        let value = builder.new_local(ty, None);
        builder.assign(
            Place::Local(value),
            Rvalue::Call {
                callee: Callee {
                    def: callee,
                    args: vec![],
                    ret: ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        if escapes {
            builder.assign(
                Place::Global(crate::Global(0)),
                Rvalue::Use(Operand::Copy(Place::Local(value))),
            );
        }
        builder.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
        builder.finish()
    };
    for escapes in [false, true] {
        for reverse in [false, true] {
            let mut functions = vec![
                build(a, b, escapes),
                build(b, a, false),
                build(caller, b, false),
            ];
            if reverse {
                functions.reverse();
            }
            let mir = Mir {
                functions,
                globals: vec![crate::MirGlobal {
                    id: crate::Global(0),
                    ty,
                }],
                ..Default::default()
            };
            let safety = compute_safety(&mir, &ctx.interner, &ctor_only_defs(&mir));
            for def in [a, b, caller] {
                assert_eq!(safety[&(def, vec![])], !escapes);
            }
        }
    }
}

#[test]
fn wraps_unique_call_that_only_news_del_free_class() {
    let mut ctx = TypeCtx::new();
    let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
    let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
    let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
    let mut layouts = LayoutTable::default();
    layouts.insert(ty, layout);

    let mut alloc = FunctionBuilder::new("alloc_node", ty);
    alloc.set_def(alloc_def, vec![]);
    let t = alloc.new_local(ty, Some("t".into()));
    alloc.assign(
        Place::Local(t),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
    drop_it.set_def(drop_def, vec![]);
    let x = drop_it.new_local(ty, Some("x".into()));
    drop_it.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: alloc_def,
                args: vec![],
                ret: ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    drop_it.push(Statement::ReleaseUnique(Operand::Copy(Place::Local(x))));
    drop_it.terminate(Terminator::Return(None));

    let mut mir = Mir {
        functions: vec![alloc.finish(), drop_it.finish()],
        layouts,
        ..Default::default()
    };
    assert!(UniqueRegion.run(&mut mir, &ctx.interner));
    let drop_fn = &mir.functions[1];
    assert!(
        matches!(drop_fn.blocks[0].stmts[0], Statement::RegionEnter),
        "{:?}",
        drop_fn.blocks[0].stmts
    );
    assert!(
        drop_fn.blocks[0]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::RegionLeave)),
        "{:?}",
        drop_fn.blocks[0].stmts
    );
    assert!(!drop_fn.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::ReleaseUnique(_))),);
}

#[test]
fn wraps_switch_join_of_unique_call() {
    use crate::{BinOp, BlockId};

    let mut ctx = TypeCtx::new();
    let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
    let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
    let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
    let mut layouts = LayoutTable::default();
    layouts.insert(ty, layout);

    let mut alloc = FunctionBuilder::new("alloc_node", ty);
    alloc.set_def(alloc_def, vec![]);
    let t = alloc.new_local(ty, Some("t".into()));
    alloc.assign(
        Place::Local(t),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
    drop_it.set_def(drop_def, vec![]);
    let x = drop_it.new_local(ty, Some("x".into()));
    let acc = drop_it.new_local(ctx.interner.int(), Some("acc".into()));
    drop_it.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: alloc_def,
                args: vec![],
                ret: ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    let some = drop_it.new_block();
    let none = drop_it.new_block();
    let join = drop_it.new_block();
    drop_it.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(x)),
        then_blk: some,
        else_blk: none,
    });
    drop_it.switch_to(some);
    drop_it.assign(
        Place::Local(acc),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(acc)),
            Operand::Const(Const::Int(1)),
        ),
    );
    drop_it.terminate(Terminator::Goto(join));
    drop_it.switch_to(none);
    drop_it.terminate(Terminator::Goto(join));
    drop_it.switch_to(join);
    drop_it.terminate(Terminator::Return(None));

    let mut mir = Mir {
        functions: vec![alloc.finish(), drop_it.finish()],
        layouts,
        ..Default::default()
    };
    assert!(UniqueRegion.run(&mut mir, &ctx.interner));
    let drop_fn = &mir.functions[1];
    assert!(
        matches!(drop_fn.blocks[0].stmts[0], Statement::RegionEnter),
        "{:?}",
        drop_fn.blocks[0].stmts
    );
    let join_id = BlockId(3);
    assert!(
        matches!(drop_fn.block(join_id).stmts[0], Statement::RegionLeave),
        "{:?}",
        drop_fn.block(join_id).stmts
    );
}

#[test]
fn does_not_wrap_switch_join_when_phi_used_after() {
    let mut ctx = TypeCtx::new();
    let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
    let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
    let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
    let mut layouts = LayoutTable::default();
    layouts.insert(ty, layout);

    let mut alloc = FunctionBuilder::new("alloc_node", ty);
    alloc.set_def(alloc_def, vec![]);
    let t = alloc.new_local(ty, Some("t".into()));
    alloc.assign(
        Place::Local(t),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
    drop_it.set_def(drop_def, vec![]);
    let x = drop_it.new_local(ty, Some("x".into()));
    let other = drop_it.new_local(ty, Some("other".into()));
    let phi = drop_it.new_local(ty, Some("phi".into()));
    let s = drop_it.new_local(ctx.interner.string(), Some("s".into()));
    drop_it.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: alloc_def,
                args: vec![],
                ret: ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    let some = drop_it.new_block();
    let none = drop_it.new_block();
    let join = drop_it.new_block();
    drop_it.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(x)),
        then_blk: some,
        else_blk: none,
    });
    drop_it.switch_to(some);
    drop_it.assign(
        Place::Local(phi),
        Rvalue::Use(Operand::Copy(Place::Local(x))),
    );
    drop_it.terminate(Terminator::Goto(join));
    drop_it.switch_to(none);
    drop_it.assign(
        Place::Local(phi),
        Rvalue::Use(Operand::Copy(Place::Local(other))),
    );
    drop_it.terminate(Terminator::Goto(join));
    drop_it.switch_to(join);
    drop_it.assign(
        Place::Local(s),
        Rvalue::ToString(Operand::Copy(Place::Local(phi))),
    );
    drop_it.terminate(Terminator::Return(None));

    let mut mir = Mir {
        functions: vec![alloc.finish(), drop_it.finish()],
        layouts,
        ..Default::default()
    };
    UniqueRegion.run(&mut mir, &ctx.interner);
    let drop_fn = &mir.functions[1];
    assert!(
        !drop_fn
            .blocks
            .iter()
            .any(|b| b.stmts.iter().any(|s| matches!(s, Statement::RegionEnter))),
        "{:?}",
        drop_fn.blocks
    );
}

#[test]
fn strip_escaped_drops_leave_before_payload_use() {
    let mut ctx = TypeCtx::new();
    let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
    let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
    let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
    let mut layouts = LayoutTable::default();
    layouts.insert(ty, layout);

    let mut alloc = FunctionBuilder::new("alloc_node", ty);
    alloc.set_def(alloc_def, vec![]);
    let t = alloc.new_local(ty, Some("t".into()));
    alloc.assign(
        Place::Local(t),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
    drop_it.set_def(drop_def, vec![]);
    let x = drop_it.new_local(ty, Some("x".into()));
    let s = drop_it.new_local(ctx.interner.string(), Some("s".into()));
    drop_it.push(Statement::RegionEnter);
    drop_it.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: alloc_def,
                args: vec![],
                ret: ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    drop_it.push(Statement::RegionLeave);
    drop_it.assign(
        Place::Local(s),
        Rvalue::ToString(Operand::Copy(Place::Local(x))),
    );
    drop_it.terminate(Terminator::Return(None));

    let mut mir = Mir {
        functions: vec![alloc.finish(), drop_it.finish()],
        layouts,
        ..Default::default()
    };
    assert!(strip_escaped_regions(&mut mir, &ctx.interner));
    let drop_fn = &mir.functions[1];
    assert!(
        !drop_fn.blocks.iter().any(|b| b
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::RegionEnter | Statement::RegionLeave))),
        "{:?}",
        drop_fn.blocks[0].stmts
    );
}

/// `make_tree` from `bench_binary_trees`: the base case returns a niche `None`, which niche
/// canonicalization lowers to `none = null; return none`.
fn make_tree_module(ctx: &mut TypeCtx, base_returns_param: bool) -> Mir {
    use crate::BinOp;

    let node_def = ctx.register(DefKind::Struct, "TreeNode", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let int = ctx.interner.int();
    let bool_ty = ctx.interner.bool();
    let make_def = ctx.register(DefKind::Function, "make_tree", vec![]);
    let bench_def = ctx.register(DefKind::Function, "bench_binary_trees", vec![]);
    let layout = TypeLayout::from_fields(
        &ctx.interner,
        "TreeNode",
        vec![
            ("left".to_string(), ty, false, false),
            ("right".to_string(), ty, false, false),
        ],
    );
    let mut layouts = LayoutTable::default();
    layouts.insert(ty, layout);
    let callee = |arg_ty: TypeId| Callee {
        def: make_def,
        args: vec![],
        ret: arg_ty,
        take_params: vec![false],
    };

    let mut mk = FunctionBuilder::new("make_tree", ty);
    mk.set_def(make_def, vec![]);
    let depth = mk.new_param(int, Some("depth".into()));
    let spare = if base_returns_param {
        Some(mk.new_param(ty, Some("spare".into())))
    } else {
        None
    };
    let none = mk.new_local(ty, Some("none".into()));
    let c = mk.new_temp(bool_ty);
    let d1 = mk.new_temp(int);
    let l = mk.new_temp(ty);
    let r = mk.new_temp(ty);
    let node = mk.new_temp(ty);
    let some = mk.new_temp(ty);
    mk.assign(Place::Local(none), Rvalue::Use(Operand::Const(Const::Null)));
    mk.assign(
        Place::Local(c),
        Rvalue::Binary(
            BinOp::Le,
            Operand::Copy(Place::Local(depth)),
            Operand::Const(Const::Int(0)),
        ),
    );
    let base = mk.new_block();
    let rec = mk.new_block();
    mk.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: base,
        else_blk: rec,
    });
    mk.switch_to(base);
    let base_ret = match spare {
        Some(p) => p,
        None => none,
    };
    mk.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
        base_ret,
    )))));
    mk.switch_to(rec);
    mk.assign(
        Place::Local(d1),
        Rvalue::Binary(
            BinOp::Sub,
            Operand::Copy(Place::Local(depth)),
            Operand::Const(Const::Int(1)),
        ),
    );
    let mut rec_args = vec![Operand::Copy(Place::Local(d1))];
    if let Some(p) = spare {
        rec_args.push(Operand::Copy(Place::Local(p)));
    }
    for dst in [l, r] {
        mk.assign(
            Place::Local(dst),
            Rvalue::Call {
                callee: callee(ty),
                args: rec_args.clone(),
            },
        );
    }
    mk.assign(
        Place::Local(node),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    for (field, src) in [(0, l), (1, r)] {
        mk.assign(
            Place::Field { base: node, field },
            Rvalue::Move { src, cast: None },
        );
    }
    mk.assign(
        Place::Local(some),
        Rvalue::Use(Operand::Copy(Place::Local(node))),
    );
    mk.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(some)))));

    let mut bench = FunctionBuilder::new("bench_binary_trees", ctx.interner.void());
    bench.set_def(bench_def, vec![]);
    let x = bench.new_local(ty, Some("x".into()));
    let mut args = vec![Operand::Const(Const::Int(12))];
    if base_returns_param {
        args.push(Operand::Const(Const::Null));
    }
    bench.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: callee(ty),
            args,
        },
    );
    bench.push(Statement::ReleaseUnique(Operand::Copy(Place::Local(x))));
    bench.terminate(Terminator::Return(None));

    Mir {
        functions: vec![mk.finish(), bench.finish()],
        layouts,
        ..Default::default()
    }
}

fn has_region_enter(f: &MirFunction) -> bool {
    f.blocks
        .iter()
        .any(|b| b.stmts.iter().any(|s| matches!(s, Statement::RegionEnter)))
}

#[test]
fn wraps_builder_whose_base_case_returns_niche_none() {
    let mut ctx = TypeCtx::new();
    let mut mir = make_tree_module(&mut ctx, false);
    assert!(UniqueRegion.run(&mut mir, &ctx.interner));
    let bench = &mir.functions[1];
    assert!(
        matches!(bench.blocks[0].stmts[0], Statement::RegionEnter),
        "{:?}",
        bench.blocks[0].stmts
    );
    assert!(
        matches!(bench.blocks[0].stmts[2], Statement::RegionLeave),
        "{:?}",
        bench.blocks[0].stmts
    );
    assert!(!has_region_enter(&mir.functions[0]));
}

#[test]
fn does_not_wrap_builder_that_may_return_a_parameter() {
    let mut ctx = TypeCtx::new();
    let mut mir = make_tree_module(&mut ctx, true);
    assert!(!UniqueRegion.run(&mut mir, &ctx.interner));
    assert!(!has_region_enter(&mir.functions[1]));
}

#[test]
fn returns_fresh_requires_every_definition_fresh() {
    let mut ctx = TypeCtx::new();
    let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let mut f = FunctionBuilder::new("pick", ty);
    let p = f.new_param(ty, Some("p".into()));
    let v = f.new_local(ty, Some("v".into()));
    f.assign(
        Place::Local(v),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    f.assign(Place::Local(v), Rvalue::Use(Operand::Copy(Place::Local(p))));
    f.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(v)))));
    assert!(!returns_fresh(&ctx.interner, &f.finish()));
}

#[test]
fn strip_escaped_keeps_region_when_only_pre_region_locals_are_used_after() {
    let mut ctx = TypeCtx::new();
    let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node_def, vec![]);
    let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
    let mut f = FunctionBuilder::new("drop_it", ctx.interner.void());
    let sw = f.new_local(ty, Some("sw".into()));
    let x = f.new_local(ty, Some("x".into()));
    let s = f.new_local(ctx.interner.string(), Some("s".into()));
    f.assign(
        Place::Local(sw),
        Rvalue::New {
            def: node_def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    f.push(Statement::RegionEnter);
    f.assign(
        Place::Local(x),
        Rvalue::Call {
            callee: Callee {
                def: alloc_def,
                args: vec![],
                ret: ty,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    f.push(Statement::RegionLeave);
    f.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
    f.assign(
        Place::Local(s),
        Rvalue::ToString(Operand::Copy(Place::Local(sw))),
    );
    f.terminate(Terminator::Return(None));
    let mut mir = Mir {
        functions: vec![f.finish()],
        ..Default::default()
    };
    assert!(!strip_escaped_regions(&mut mir, &ctx.interner));
    assert!(has_region_enter(&mir.functions[0]));
}
