//! Gate batching for private builders whose complete call graph creates only fresh,
//! destructor-free references. Scalar arguments cannot import an existing ownership graph.
use crate::{AllocPolicy, Const, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeInterner};
use std::collections::BTreeSet;

pub(super) fn run(mir: &mut Mir, interner: &TypeInterner) {
    let mut safe: BTreeSet<_> = mir
        .functions
        .iter()
        .filter(|f| {
            !f.is_async
                && f.instance.is_empty()
                && interner.is_rc_tracked(f.ret)
                && f.params.iter().all(|p| {
                    !interner.is_rc_tracked(f.local_ty(*p)) && !f.locals[p.0 as usize].is_ref
                })
                && f.locals.iter().all(|l| !interner.is_value_type(l.ty))
        })
        .map(|f| f.def)
        .collect();
    loop {
        let rejected: Vec<_> = mir
            .functions
            .iter()
            .filter(|f| safe.contains(&f.def) && !body_safe(f, mir, interner, &safe))
            .map(|f| f.def)
            .collect();
        if rejected.is_empty() {
            break;
        }
        for def in rejected {
            safe.remove(&def);
        }
    }
    let mut managed: BTreeSet<_> = mir
        .functions
        .iter()
        .filter(|f| safe.contains(&f.def))
        .filter(|f| {
            f.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
                matches!(s, Statement::Assign(_, Rvalue::New { ty, .. })
                if crate::ownership::cycle_capable(&mir.layouts, interner, *ty))
            })
        })
        .map(|f| f.def)
        .collect();
    loop {
        let callers: Vec<_> = mir.functions.iter().filter(|f| safe.contains(&f.def) && !managed.contains(&f.def))
            .filter(|f| f.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
                matches!(s, Statement::Assign(_, Rvalue::Call { callee, .. }) if managed.contains(&callee.def))
            })).map(|f| f.def).collect();
        if callers.is_empty() {
            break;
        }
        managed.extend(callers);
    }
    let mut private = safe.clone();
    loop {
        let rejected: Vec<_> = mir
            .functions
            .iter()
            .filter(|f| private.contains(&f.def))
            .filter(|f| {
                f.blocks.iter().flat_map(|b| &b.stmts).any(|s| match s {
                    Statement::Assign(_, Rvalue::New { policy, .. }) => {
                        *policy != AllocPolicy::Private
                    }
                    Statement::Assign(_, Rvalue::Call { callee, .. }) => {
                        !private.contains(&callee.def)
                    }
                    _ => false,
                })
            })
            .map(|f| f.def)
            .collect();
        if rejected.is_empty() {
            break;
        }
        for def in rejected {
            private.remove(&def);
        }
    }
    for f in &mut mir.functions {
        f.batched_construction = (safe.contains(&f.def) && managed.contains(&f.def)).then_some(
            if private.contains(&f.def) {
                AllocPolicy::Private
            } else {
                AllocPolicy::Tracked
            },
        );
    }
}

fn local_or_constant(op: &Operand) -> bool {
    matches!(op, Operand::Copy(Place::Local(_)))
        || matches!(op, Operand::Const(c) if !matches!(c, Const::Str(_)))
}

fn body_safe(f: &MirFunction, mir: &Mir, interner: &TypeInterner, safe: &BTreeSet<DefId>) -> bool {
    let mut constructs = false;
    for block in &f.blocks {
        for statement in &block.stmts {
            match statement {
                Statement::SourceLine(_) | Statement::Nop => {}
                Statement::Retain(op) | Statement::Release(op) if local_or_constant(op) => {}
                Statement::Assign(Place::Local(dest), value) => {
                    let scalar = !interner.is_rc_tracked(f.local_ty(*dest));
                    let valid = match value {
                        Rvalue::Use(op) => match op {
                            Operand::Copy(Place::Field { base, field }) => mir
                                .layouts
                                .get(f.local_ty(*base))
                                .and_then(|layout| layout.fields.get(*field))
                                .is_some_and(|field| !field.is_weak && !field.is_unowned),
                            _ => local_or_constant(op),
                        },
                        Rvalue::UnionField { base, ty, .. } => {
                            interner.is_niche_union(*ty) && local_or_constant(base)
                        }
                        Rvalue::Move { cast: None, .. } => true,
                        Rvalue::Binary(_, a, b) if scalar => {
                            local_or_constant(a) && local_or_constant(b)
                        }
                        Rvalue::Unary(_, a) if scalar => local_or_constant(a),
                        Rvalue::New { ty, ctor, args, .. } => {
                            constructs = true;
                            (match ctor {
                                Some(ctor) => {
                                    ctor.batched || null_initializer(mir, interner, ctor.def, *ty)
                                }
                                None => args.is_empty(),
                            }) && !interner.is_shared_type(*ty)
                                && mir.layouts.get(*ty).is_some_and(|s| {
                                    !s.has_destructor()
                                        && s.fields
                                            .iter()
                                            .all(|field| !interner.is_value_type(field.ty))
                                })
                                && args.iter().all(local_or_constant)
                        }
                        Rvalue::Call { callee, args } => {
                            constructs = true;
                            callee.args.is_empty()
                                && safe.contains(&callee.def)
                                && args.iter().all(local_or_constant)
                        }
                        Rvalue::UnionNew { ty, args, .. } => {
                            interner.is_niche_union(*ty) && args.iter().all(local_or_constant)
                        }
                        _ => false,
                    };
                    if !valid {
                        return false;
                    }
                }
                Statement::Assign(Place::Field { .. }, value) => {
                    let valid = match value {
                        Rvalue::Use(op) => local_or_constant(op),
                        Rvalue::Move { cast: None, .. } => true,
                        Rvalue::UnionNew { ty, args, .. } => {
                            interner.is_niche_union(*ty) && args.iter().all(local_or_constant)
                        }
                        _ => false,
                    };
                    if !valid {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        match &block.terminator {
            Terminator::Goto(_) | Terminator::Unreachable => {}
            Terminator::If { cond, .. } if local_or_constant(cond) => {}
            Terminator::Switch { value, .. } if local_or_constant(value) => {}
            Terminator::Return(Some(op)) if local_or_constant(op) => {}
            _ => return false,
        }
    }
    constructs
}

// Null weak fields create no handle. Their ordinary constructor remains valid under an
// outer builder gate; nonnull weak operations and callbacks never qualify here.
pub(super) fn null_initializer(
    mir: &Mir,
    interner: &TypeInterner,
    def: DefId,
    ty: dream_types::TypeId,
) -> bool {
    let Some(f) = mir
        .functions
        .iter()
        .find(|f| f.def == def && f.instance.is_empty())
    else {
        return false;
    };
    if f.is_async || f.params.len() != 1 || f.local_ty(f.params[0]) != ty || f.blocks.len() != 1 {
        return false;
    }
    let this = f.params[0];
    matches!(f.blocks[0].terminator, Terminator::Return(None))
        && f.blocks[0].stmts.iter().all(|s| match s {
            Statement::SourceLine(_) | Statement::Nop => true,
            Statement::Assign(Place::Field { base, .. }, value) if *base == this => match value {
                Rvalue::Use(Operand::Const(Const::Null)) => true,
                Rvalue::UnionNew {
                    ty, variant, args, ..
                } => {
                    interner.is_niche_union(*ty)
                        && args.is_empty()
                        && mir
                            .layouts
                            .unions
                            .get(ty)
                            .and_then(|layout| layout.variants.get(*variant))
                            .is_some_and(|variant| variant.fields.is_empty())
                }
                _ => false,
            },
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewCtor, build::FunctionBuilder};
    use dream_hir::TypeLayout;
    use dream_types::{DefKind, TypeCtx};

    #[test]
    fn only_builders_with_transitive_cycle_managed_allocations_acquire_the_gate() {
        for recursive in [false, true] {
            let mut ctx = TypeCtx::new();
            let def = ctx.register(DefKind::Struct, "Node", vec![]);
            let factory = ctx.register(DefKind::Function, "factory", vec![]);
            let wrapper = ctx.register(DefKind::Function, "wrapper", vec![]);
            let ty = ctx.interner.struct_ty(def, vec![]);
            let mut mir = Mir::default();
            mir.layouts.insert(
                ty,
                TypeLayout::from_fields(
                    &ctx.interner,
                    "Node",
                    [(
                        "field".into(),
                        if recursive { ty } else { ctx.interner.int() },
                        false,
                        false,
                    )],
                ),
            );
            let mut b = FunctionBuilder::new("factory", ty);
            b.set_def(factory, vec![]);
            let result = b.new_local(ty, None);
            b.assign(
                Place::Local(result),
                Rvalue::New {
                    def,
                    ty,
                    ctor: None,
                    args: vec![],
                    policy: AllocPolicy::Tracked,
                },
            );
            b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
                result,
            )))));
            mir.functions.push(b.finish());
            let mut b = FunctionBuilder::new("wrapper", ty);
            b.set_def(wrapper, vec![]);
            let result = b.new_local(ty, None);
            b.assign(
                Place::Local(result),
                Rvalue::Call {
                    callee: crate::Callee {
                        def: factory,
                        args: vec![],
                        ret: ty,
                        take_params: vec![],
                    },
                    args: vec![],
                },
            );
            b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
                result,
            )))));
            mir.functions.push(b.finish());
            run(&mut mir, &ctx.interner);
            for f in &mir.functions {
                assert_eq!(
                    f.batched_construction,
                    recursive.then_some(AllocPolicy::Tracked)
                );
            }
        }
    }

    #[test]
    fn fresh_weak_forests_batch_only_when_no_weak_observation_can_see_delayed_cleanup() {
        for reads_weak in [false, true] {
            let mut ctx = TypeCtx::new();
            let def = ctx.register(DefKind::Struct, "Node", vec![]);
            let ctor = ctx.register(DefKind::Function, "constructor", vec![]);
            let ty = ctx.interner.struct_ty(def, vec![]);
            let mut mir = Mir::default();
            mir.layouts.insert(
                ty,
                TypeLayout::from_fields(
                    &ctx.interner,
                    "Node",
                    [
                        ("child".into(), ty, false, false),
                        ("parent".into(), ty, true, false),
                    ],
                ),
            );
            let mut init = FunctionBuilder::new("constructor", ctx.interner.void());
            init.set_def(ctor, vec![]);
            let this = init.new_param(ty, None);
            for field in 0..2 {
                init.assign(
                    Place::Field { base: this, field },
                    Rvalue::Use(Operand::Const(Const::Null)),
                );
            }
            init.terminate(Terminator::Return(None));
            mir.functions.push(init.finish());
            let mut builder = FunctionBuilder::new("forest", ty);
            let parent = builder.new_local(ty, None);
            let child = builder.new_local(ty, None);
            for local in [parent, child] {
                builder.assign(
                    Place::Local(local),
                    Rvalue::New {
                        def,
                        ty,
                        ctor: Some(NewCtor {
                            def: ctor,
                            take_params: vec![],
                            batched: false,
                        }),
                        args: vec![],
                        policy: AllocPolicy::Tracked,
                    },
                );
            }
            builder.assign(
                Place::Field {
                    base: child,
                    field: 1,
                },
                Rvalue::Use(Operand::Copy(Place::Local(parent))),
            );
            builder.assign(
                Place::Field {
                    base: parent,
                    field: 0,
                },
                Rvalue::Use(Operand::Copy(Place::Local(child))),
            );
            if reads_weak {
                let observation = builder.new_local(ty, None);
                builder.assign(
                    Place::Local(observation),
                    Rvalue::Use(Operand::Copy(Place::Field {
                        base: child,
                        field: 1,
                    })),
                );
            }
            builder.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
                parent,
            )))));
            mir.functions.push(builder.finish());
            run(&mut mir, &ctx.interner);
            assert_eq!(
                mir.functions[1].batched_construction,
                (!reads_weak).then_some(AllocPolicy::Tracked)
            );
        }
    }

    #[test]
    fn builder_gate_requires_fresh_private_graphs_without_observable_calls() {
        for variant in 0..5 {
            let mut ctx = TypeCtx::new();
            let def = ctx.register(DefKind::Struct, "Node", vec![]);
            let ctor = ctx.register(DefKind::Function, "constructor", vec![]);
            let ty = ctx.interner.struct_ty(def, vec![]);
            let mut mir = Mir::default();
            let mut layout = TypeLayout::from_fields(
                &ctx.interner,
                "Node",
                [("child".into(), ty, false, false)],
            );
            if variant == 4 {
                layout.destructor = Some(ctor);
            }
            mir.layouts.insert(ty, layout);
            let mut b = FunctionBuilder::new("builder", ty);
            if variant == 2 {
                b.new_param(ty, None);
            }
            let result = b.new_local(ty, None);
            b.assign(
                Place::Local(result),
                Rvalue::New {
                    def,
                    ty,
                    ctor: Some(NewCtor {
                        def: ctor,
                        take_params: vec![false, true],
                        batched: variant != 3,
                    }),
                    args: vec![Operand::Const(Const::Null)],
                    policy: AllocPolicy::Private,
                },
            );
            if variant == 1 {
                b.push(Statement::Print {
                    arg: Operand::Const(Const::Int(1)),
                    ty: ctx.interner.int(),
                    newline: true,
                });
            }
            b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
                result,
            )))));
            mir.functions.push(b.finish());
            run(&mut mir, &ctx.interner);
            assert_eq!(
                mir.functions[0].batched_construction.is_some(),
                variant == 0
            );
        }
    }
}
