use crate::Mir;
use crate::analysis::effects::ModRefTable;
use dream_hir::{InterfaceImpl, InterfaceInfo, TypeLayout};
use dream_types::{DefKind, TypeCtx};

#[test]
fn interface_cleanup_uses_the_complete_concrete_implementation_set() {
    let mut ctx = TypeCtx::new();
    let interface = ctx.register(DefKind::Interface, "I", vec![]);
    let a = ctx.register(DefKind::Struct, "A", vec![]);
    let b = ctx.register(DefKind::Struct, "B", vec![]);
    let iface = ctx.interner.interface_ty(interface, vec![]);
    let a = ctx.interner.struct_ty(a, vec![]);
    let b = ctx.interner.struct_ty(b, vec![]);
    let mut mir = Mir::default();
    mir.interfaces.interfaces.push(InterfaceInfo {
        ty: iface,
        name: "I".into(),
        method_count: 0,
        sigs: vec![],
    });
    mir.interfaces.impls.push(InterfaceImpl {
        class_ty: a,
        entries: vec![(0, vec![])],
    });
    mir.layouts.insert(a, TypeLayout::default());
    mir.layouts.insert(
        b,
        TypeLayout {
            destructor: Some(dream_types::DefId::root(100)),
            ..Default::default()
        },
    );
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(!table.may_run_del(iface, &ctx.interner, &mir.layouts));
    assert!(table.may_run_del(ctx.interner.object(), &ctx.interner, &mir.layouts));
    mir.interfaces.impls.push(InterfaceImpl {
        class_ty: b,
        entries: vec![(0, vec![])],
    });
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(table.may_run_del(iface, &ctx.interner, &mir.layouts));
    mir.interfaces.impls.clear();
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(table.may_run_del(iface, &ctx.interner, &mir.layouts));
}

#[test]
fn interface_effects_reach_direct_callers_constructors_and_destructors() {
    use crate::build::FunctionBuilder;
    use crate::{Callee, Const, Operand, Place, Rvalue, Statement, Terminator};
    use dream_types::DefId;
    use std::collections::BTreeSet;
    for opaque in [false, true] {
        let mut ctx = TypeCtx::new();
        let interface = ctx.register(DefKind::Interface, "I", vec![]);
        let holder = ctx.register(DefKind::Struct, "Holder", vec![]);
        let implementation = ctx.register(DefKind::Struct, "Impl", vec![]);
        let auxiliary = ctx.register(DefKind::Struct, "Auxiliary", vec![]);
        let iface = ctx.interner.interface_ty(interface, vec![]);
        let holder = ctx.interner.struct_ty(holder, vec![]);
        let implementation = ctx.interner.struct_ty(implementation, vec![]);
        let auxiliary = ctx.interner.struct_ty(auxiliary, vec![]);
        let void = ctx.interner.void();
        let sig = ctx.interner.func(vec![holder], void);
        let mut method = FunctionBuilder::new("method", void);
        method.set_def(DefId::root(10), vec![]);
        method.new_param(implementation, None);
        let h = method.new_param(holder, None);
        if opaque {
            method.push(Statement::Call {
                callee: Callee {
                    def: DefId::root(999),
                    args: vec![],
                    ret: void,
                    take_params: vec![],
                },
                args: vec![],
            });
        } else {
            method.assign(
                Place::Field { base: h, field: 0 },
                Rvalue::Use(Operand::Const(Const::Null)),
            );
        }
        method.terminate(Terminator::Return(None));
        let mut dispatch = FunctionBuilder::new("dispatch", void);
        dispatch.set_def(DefId::root(11), vec![]);
        let receiver = dispatch.new_param(iface, None);
        let h = dispatch.new_param(holder, None);
        dispatch.push(Statement::InterfaceCall {
            receiver: Operand::Copy(Place::Local(receiver)),
            iface_id: 0,
            method_slot: 0,
            sig,
            args: vec![Operand::Copy(Place::Local(h))],
        });
        dispatch.terminate(Terminator::Return(None));
        let mut functions = vec![];
        for (def, callee) in [(13, 12), (12, 11)] {
            let mut b = FunctionBuilder::new("caller", void);
            b.set_def(DefId::root(def), vec![]);
            let receiver = b.new_param(iface, None);
            let h = b.new_param(holder, None);
            b.push(Statement::Call {
                callee: Callee {
                    def: DefId::root(callee),
                    args: vec![],
                    ret: void,
                    take_params: vec![false, false],
                },
                args: vec![
                    Operand::Copy(Place::Local(receiver)),
                    Operand::Copy(Place::Local(h)),
                ],
            });
            b.terminate(Terminator::Return(None));
            functions.push(b.finish());
        }
        functions.extend([dispatch.finish(), method.finish()]);
        let mut mir = Mir {
            functions,
            ..Mir::default()
        };
        mir.interfaces.interfaces.push(InterfaceInfo {
            ty: iface,
            name: "I".into(),
            method_count: 1,
            sigs: vec![sig],
        });
        mir.interfaces.impls.push(InterfaceImpl {
            class_ty: implementation,
            entries: vec![(0, vec![Some(DefId::root(10))])],
        });
        mir.layouts.insert(
            holder,
            TypeLayout::from_fields(
                &ctx.interner,
                "Holder",
                [("value".into(), ctx.interner.string(), false, false)],
            ),
        );
        let mut layout = TypeLayout::from_fields(
            &ctx.interner,
            "Auxiliary",
            [
                ("op".into(), iface, false, false),
                ("holder".into(), holder, false, false),
            ],
        );
        layout.destructor = Some(DefId::root(14));
        mir.layouts.insert(auxiliary, layout);
        for def in [14, 15] {
            let mut b = FunctionBuilder::new("auxiliary_member", void);
            b.set_def(DefId::root(def), vec![]);
            let this = b.new_param(auxiliary, None);
            let receiver;
            let h;
            if def == 14 {
                receiver = b.new_local(iface, None);
                h = b.new_local(holder, None);
                b.assign(
                    Place::Local(receiver),
                    Rvalue::Use(Operand::Copy(Place::Field {
                        base: this,
                        field: 0,
                    })),
                );
                b.assign(
                    Place::Local(h),
                    Rvalue::Use(Operand::Copy(Place::Field {
                        base: this,
                        field: 1,
                    })),
                );
            } else {
                receiver = b.new_param(iface, None);
                h = b.new_param(holder, None);
                b.assign(
                    Place::Field {
                        base: this,
                        field: 0,
                    },
                    Rvalue::Use(Operand::Copy(Place::Local(receiver))),
                );
                b.assign(
                    Place::Field {
                        base: this,
                        field: 1,
                    },
                    Rvalue::Use(Operand::Copy(Place::Local(h))),
                );
            }
            b.push(Statement::Call {
                callee: Callee {
                    def: DefId::root(13),
                    args: vec![],
                    ret: void,
                    take_params: vec![false, false],
                },
                args: vec![
                    Operand::Copy(Place::Local(receiver)),
                    Operand::Copy(Place::Local(h)),
                ],
            });
            b.terminate(Terminator::Return(None));
            mir.functions.insert(0, b.finish());
        }
        mir.layouts.insert(implementation, TypeLayout::default());
        let table = ModRefTable::compute(&mir, &ctx.interner);
        let fields = BTreeSet::from([(holder, 0)]);
        for def in [11, 12, 13] {
            assert!(
                table
                    .call_def(DefId::root(def), &[])
                    .hits(&fields, &BTreeSet::new()),
                "call {def}, opaque={opaque}"
            );
        }
        assert!(
            table.ctor(DefId::root(15)).hits(&fields, &BTreeSet::new()),
            "constructor, opaque={opaque}"
        );
        assert!(
            table.del().hits(&fields, &BTreeSet::new()),
            "destructor, opaque={opaque}"
        );
    }
}

#[test]
fn interface_slot_joins_every_concrete_function_instance() {
    use crate::build::FunctionBuilder;
    use crate::{Const, Operand, Place, Rvalue, Terminator};
    use dream_types::DefId;
    use std::collections::BTreeSet;
    let mut ctx = TypeCtx::new();
    let interface = ctx.register(DefKind::Interface, "I", vec![]);
    let holder = ctx.register(DefKind::Struct, "Holder", vec![]);
    let generic = ctx.register(DefKind::Struct, "Generic", vec!["T".into()]);
    let iface = ctx.interner.interface_ty(interface, vec![]);
    let holder = ctx.interner.struct_ty(holder, vec![]);
    let void = ctx.interner.void();
    let sig = ctx.interner.func(vec![holder], void);
    let mut mir = Mir::default();
    mir.interfaces.interfaces.push(InterfaceInfo {
        ty: iface,
        name: "I".into(),
        method_count: 1,
        sigs: vec![sig],
    });
    for (ty, stores) in [(ctx.interner.int(), true), (ctx.interner.string(), false)] {
        let class_ty = ctx.interner.struct_ty(generic, vec![ty]);
        let mut b = FunctionBuilder::new("method", void);
        b.set_def(DefId::root(10), vec![ty]);
        b.new_param(class_ty, None);
        let h = b.new_param(holder, None);
        if stores {
            b.assign(
                Place::Field { base: h, field: 0 },
                Rvalue::Use(Operand::Const(Const::Null)),
            );
        }
        b.terminate(Terminator::Return(None));
        mir.functions.push(b.finish());
        mir.layouts.insert(class_ty, TypeLayout::default());
        mir.interfaces.impls.push(InterfaceImpl {
            class_ty,
            entries: vec![(0, vec![Some(DefId::root(10))])],
        });
    }
    mir.layouts.insert(
        holder,
        TypeLayout::from_fields(
            &ctx.interner,
            "Holder",
            [("value".into(), ctx.interner.string(), false, false)],
        ),
    );
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(
        table
            .iface(0, 0)
            .hits(&BTreeSet::from([(holder, 0)]), &BTreeSet::new())
    );
}

#[test]
fn a_method_that_can_drop_a_destructor_owner_is_not_quiet() {
    use crate::build::FunctionBuilder;
    use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};
    use dream_types::DefId;
    let mut ctx = TypeCtx::new();
    let interface = ctx.register(DefKind::Interface, "I", vec![]);
    let implementation = ctx.register(DefKind::Struct, "Impl", vec![]);
    let holder = ctx.register(DefKind::Struct, "Holder", vec![]);
    let dangerous = ctx.register(DefKind::Struct, "Dangerous", vec![]);
    let iface = ctx.interner.interface_ty(interface, vec![]);
    let implementation = ctx.interner.struct_ty(implementation, vec![]);
    let holder = ctx.interner.struct_ty(holder, vec![]);
    let dangerous = ctx.interner.struct_ty(dangerous, vec![]);
    let void = ctx.interner.void();
    let sig = ctx.interner.func(vec![dangerous], void);
    let mut method = FunctionBuilder::new("consume", void);
    method.set_def(DefId::root(10), vec![]);
    method.new_param(implementation, None);
    let d = method.new_take_param(dangerous, None);
    method.push(Statement::Release(Operand::Copy(Place::Local(d))));
    method.terminate(Terminator::Return(None));
    let mut del = FunctionBuilder::new("destructor", void);
    del.set_def(DefId::root(11), vec![]);
    let this = del.new_param(dangerous, None);
    let h = del.new_local(holder, None);
    del.assign(
        Place::Local(h),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    del.assign(
        Place::Field { base: h, field: 0 },
        Rvalue::Use(Operand::Const(Const::Null)),
    );
    del.terminate(Terminator::Return(None));
    let mut mir = Mir {
        functions: vec![method.finish(), del.finish()],
        ..Mir::default()
    };
    mir.interfaces.interfaces.push(InterfaceInfo {
        ty: iface,
        name: "I".into(),
        method_count: 1,
        sigs: vec![sig],
    });
    mir.interfaces.impls.push(InterfaceImpl {
        class_ty: implementation,
        entries: vec![(0, vec![Some(DefId::root(10))])],
    });
    mir.layouts.insert(implementation, TypeLayout::default());
    mir.layouts.insert(
        holder,
        TypeLayout::from_fields(
            &ctx.interner,
            "Holder",
            [("value".into(), ctx.interner.string(), false, false)],
        ),
    );
    let mut layout = TypeLayout::from_fields(
        &ctx.interner,
        "Dangerous",
        [("holder".into(), holder, false, false)],
    );
    layout.destructor = Some(DefId::root(11));
    mir.layouts.insert(dangerous, layout);
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(!table.call_def(DefId::root(10), &[]).is_quiet());
    assert!(!table.iface(0, 0).is_quiet());
}

#[test]
fn an_uncomputed_effect_table_does_not_assume_destructor_cleanup_is_quiet() {
    assert!(!ModRefTable::default().del().is_quiet());
}

#[test]
fn a_destructor_without_an_analyzable_body_remains_opaque() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "ForeignOwner", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut mir = Mir::default();
    mir.layouts.insert(
        ty,
        TypeLayout {
            destructor: Some(dream_types::DefId::root(100)),
            ..TypeLayout::default()
        },
    );
    let effects = ModRefTable::compute(&mir, &ctx.interner);
    assert!(matches!(
        effects.del(),
        crate::analysis::effects::ModRef::Top
    ));
}

#[test]
fn observed_loads_are_ownership_observations_in_function_summaries() {
    use crate::build::FunctionBuilder;
    use crate::{Operand, Place, Rvalue, Terminator};
    use dream_types::DefId;
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Observer", vec![]);
    let observer = ctx.interner.struct_ty(def, vec![]);
    let mut b = FunctionBuilder::new("read", ctx.interner.void());
    b.set_def(DefId::root(10), vec![]);
    let this = b.new_param(observer, None);
    let result = b.new_local(ctx.interner.string(), None);
    b.assign(
        Place::Local(result),
        Rvalue::ObservedLoad(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    b.terminate(Terminator::Return(None));
    let mut mir = Mir {
        functions: vec![b.finish()],
        ..Mir::default()
    };
    mir.layouts.insert(
        observer,
        TypeLayout::from_fields(
            &ctx.interner,
            "Observer",
            [("value".into(), ctx.interner.string(), false, true)],
        ),
    );
    let effects = ModRefTable::compute(&mir, &ctx.interner);
    let summary = effects.call_def(DefId::root(10), &[]);
    assert!(
        matches!(&summary, crate::analysis::effects::ModRef::Known(known) if known.observes_rc)
    );
    assert!(!summary.is_quiet());
}

#[test]
fn a_bounds_check_panic_keeps_its_caller_quiet() {
    use crate::build::FunctionBuilder;
    use crate::{Callee, Const, Operand, Statement, Terminator};
    use dream_types::DefId;
    let ctx = TypeCtx::new();
    let panic = DefId::root(20);
    let mut b = FunctionBuilder::new("checked", ctx.interner.void());
    b.set_def(DefId::root(10), vec![]);
    b.push(Statement::Call {
        callee: Callee {
            def: panic,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![],
        },
        args: vec![Operand::Const(Const::Str("out of range".into()))],
    });
    b.terminate(Terminator::Return(None));
    let mut mir = Mir {
        functions: vec![b.finish()],
        ..Mir::default()
    };
    mir.intrinsics
        .push((panic, dream_abi::intrinsics::ATTR_PANIC.into()));
    let effects = ModRefTable::compute(&mir, &ctx.interner);
    assert!(effects.call_def(DefId::root(10), &[]).is_quiet());
}
