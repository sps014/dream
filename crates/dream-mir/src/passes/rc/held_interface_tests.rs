use super::*;
use dream_hir::{InterfaceImpl, InterfaceInfo};

#[test]
fn closed_interface_calls_bind_receiver_before_explicit_arguments() {
    for aliases in [false, true] {
        for unknown_target in [false, true] {
            let mut t = types();
            let iface = t.i.interface_ty(DefId::root(800), vec![]);
            let sig = t.i.func(vec![t.boxed], t.i.void());
            let mut method = FunctionBuilder::new("method", t.i.void());
            method.set_def(DefId::root(801), vec![]);
            method.new_param(t.boxed, None);
            let written = method.new_param(t.boxed, None);
            method.assign(
                Place::Field {
                    base: written,
                    field: 0,
                },
                Rvalue::Use(Operand::Const(Const::Null)),
            );
            method.terminate(Terminator::Return(None));
            let mut relay = FunctionBuilder::new("relay", t.i.void());
            relay.set_def(DefId::root(802), vec![]);
            let receiver = relay.new_param(iface, None);
            let written = relay.new_param(t.boxed, None);
            relay.push(Statement::InterfaceCall {
                receiver: Operand::Copy(Place::Local(receiver)),
                iface_id: 0,
                method_slot: 0,
                sig,
                args: vec![Operand::Copy(Place::Local(written))],
            });
            relay.terminate(Terminator::Return(None));
            let mut snapshot = snapshot(&t, |b, base| {
                let other = b.new_local(t.boxed, None);
                b.assign(
                    Place::Local(other),
                    if aliases {
                        Rvalue::Use(Operand::Copy(Place::Local(base)))
                    } else {
                        Rvalue::New {
                            def: DefId::root(7),
                            ty: t.boxed,
                            ctor: None,
                            args: vec![],
                        }
                    },
                );
                // The receiver aliases the protected owner. Only argument 1 is written.
                b.push(Statement::Call {
                    callee: Callee {
                        def: DefId::root(802),
                        args: vec![],
                        ret: t.i.void(),
                        take_params: vec![false; 2],
                        borrowed_result: false,
                    },
                    args: vec![
                        Operand::Copy(Place::Local(base)),
                        Operand::Copy(Place::Local(other)),
                    ],
                });
            });
            let mut mir = Mir {
                functions: vec![method.finish(), relay.finish(), peek_fn(&t, false)],
                layouts: t.layouts.clone(),
                profile: dream_abi::profile::CompileProfile::Release,
                ..Mir::default()
            };
            mir.interfaces.interfaces.push(InterfaceInfo {
                ty: iface,
                name: "Writer".into(),
                method_count: 1,
                sigs: vec![sig],
            });
            mir.interfaces.impls.push(InterfaceImpl {
                class_ty: t.boxed,
                entries: vec![(0, vec![Some(DefId::root(801))])],
            });
            if unknown_target {
                mir.interfaces.impls.push(InterfaceImpl {
                    class_ty: t.boxed,
                    entries: vec![(0, vec![None])],
                });
            }
            let effects = ModRefTable::compute(&mir, &t.i);
            run_function(&mut snapshot.func, &t.i, &t.layouts, &effects);
            assert_eq!(
                rc_ops(&snapshot.func, snapshot.x),
                if aliases || unknown_target { 2 } else { 0 }
            );
        }
    }
}
