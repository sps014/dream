use super::*;
use crate::Const;
use crate::build::FunctionBuilder;

fn local(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}
fn callee(def: u32, ret: TypeId) -> Callee {
    Callee {
        borrowed_result: false,
        def: DefId::root(def),
        args: vec![],
        ret,
        take_params: vec![],
    }
}

fn module(capturing: bool, reassign: bool) -> (Mir, TypeInterner) {
    let mut i = TypeInterner::new();
    let int = i.int();
    let func = i.func(vec![int], int);
    let mut b = FunctionBuilder::new("caller", int);
    let raw = b.new_temp(int);
    b.assign(Place::Local(raw), Rvalue::FuncRef(callee(1, int)));
    let boxed = b.new_temp(func);
    b.assign(
        Place::Local(boxed),
        Rvalue::Call {
            callee: callee(2, func),
            args: vec![local(raw), Operand::Const(Const::Int(i64::from(capturing)))],
        },
    );
    if reassign {
        b.assign(
            Place::Local(boxed),
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    }
    let target = b.new_temp(int);
    b.assign(
        Place::Local(target),
        Rvalue::Call {
            callee: callee(3, int),
            args: vec![local(boxed)],
        },
    );
    let result = b.new_temp(int);
    b.assign(
        Place::Local(result),
        Rvalue::IndirectCall {
            target: local(target),
            args: vec![Operand::Const(Const::Int(42))],
            sig: func,
        },
    );
    b.terminate(Terminator::Return(Some(local(result))));
    (
        Mir {
            functions: vec![b.finish()],
            intrinsics: vec![
                (DefId::root(2), "funcbox_new".into()),
                (DefId::root(3), "funcbox_funcidx".into()),
            ],
            ..Default::default()
        },
        i,
    )
}

#[test]
fn noncapturing_box_resolves_with_borrowed_abi() {
    let (mut mir, i) = module(false, false);
    assert!(run(&mut mir, &i));
    let Statement::Assign(_, Rvalue::Call { callee, .. }) = &mir.functions[0].blocks[0].stmts[3]
    else {
        panic!("not a direct call")
    };
    assert_eq!(callee.def, DefId::root(1));
    assert_eq!(callee.take_params, vec![false]);
    assert!(!run(&mut mir, &i));
}

#[test]
fn reassigned_boxes_stay_indirect() {
    for (capture, reassign) in [(true, true), (false, true)] {
        let (mut mir, i) = module(capture, reassign);
        assert!(!run(&mut mir, &i));
    }
}

#[test]
fn captured_box_keeps_its_environment_and_resolves_the_code() {
    let (mut mir, i) = module(true, false);
    mir.functions[0].blocks[0].stmts.insert(
        3,
        Statement::Assign(
            Place::Global(crate::Global(0)),
            Rvalue::Use(Operand::Const(Const::Long(123))),
        ),
    );
    assert!(run(&mut mir, &i));
    assert!(matches!(
        mir.functions[0].blocks[0].stmts[3],
        Statement::Assign(
            Place::Global(crate::Global(0)),
            Rvalue::Use(Operand::Const(Const::Long(123)))
        )
    ));
    assert!(matches!(
        mir.functions[0].blocks[0].stmts[4],
        Statement::Assign(_, Rvalue::Call { .. })
    ));
}

#[test]
fn callback_parameter_requires_every_direct_caller_to_agree() {
    for unknown_caller in [false, true] {
        let (mut mir, i) = module(false, false);
        let caller = &mut mir.functions[0];
        let boxed = Local(1);
        let sig = caller.local_ty(boxed);
        caller.blocks[0].stmts[3] = Statement::Assign(
            Place::Local(Local(3)),
            Rvalue::Call {
                callee: callee(4, i.int()),
                args: vec![local(boxed)],
            },
        );
        let mut b = FunctionBuilder::new("dispatch", i.int());
        b.set_def(DefId::root(4), vec![]);
        let p = b.new_param(sig, None);
        let target = b.new_temp(i.int());
        b.assign(
            Place::Local(target),
            Rvalue::Call {
                callee: callee(3, i.int()),
                args: vec![local(p)],
            },
        );
        let result = b.new_temp(i.int());
        b.assign(
            Place::Local(result),
            Rvalue::IndirectCall {
                target: local(target),
                args: vec![Operand::Const(Const::Int(42))],
                sig,
            },
        );
        b.terminate(Terminator::Return(Some(local(result))));
        mir.functions.push(b.finish());
        if unknown_caller {
            let mut b = FunctionBuilder::new("other", i.int());
            let p = b.new_param(sig, None);
            b.push(Statement::Call {
                callee: callee(4, i.int()),
                args: vec![local(p)],
            });
            b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
            mir.functions.push(b.finish());
        }
        run(&mut mir, &i);
        let direct = matches!(
            mir.functions[1].blocks[0].stmts[1],
            Statement::Assign(_, Rvalue::Call { .. })
        );
        assert_eq!(direct, !unknown_caller);
    }
}

fn forwarded_callback(depth: u32, cycle: bool) -> (Mir, TypeInterner) {
    let (mut mir, i) = module(true, false);
    mir.profile = dream_abi::profile::CompileProfile::Release;
    let sig = mir.functions[0].local_ty(Local(1));
    mir.functions[0].blocks[0].stmts[3] = Statement::Assign(
        Place::Local(Local(3)),
        Rvalue::Call {
            callee: callee(100, i.int()),
            args: vec![local(Local(1))],
        },
    );
    for index in (0..depth).rev() {
        let mut b = FunctionBuilder::new("relay", i.int());
        b.set_def(DefId::root(100 + index), vec![]);
        let parameter = b.new_param(sig, None);
        let copied = b.new_temp(sig);
        b.assign(Place::Local(copied), Rvalue::Use(local(parameter)));
        let result = b.new_temp(i.int());
        if index + 1 == depth {
            let code = b.new_temp(i.int());
            b.assign(
                Place::Local(code),
                Rvalue::Call {
                    callee: callee(3, i.int()),
                    args: vec![local(copied)],
                },
            );
            b.assign(
                Place::Local(result),
                Rvalue::IndirectCall {
                    target: local(code),
                    args: vec![Operand::Const(Const::Int(42))],
                    sig,
                },
            );
            if cycle {
                b.push(Statement::Call {
                    callee: callee(100, i.int()),
                    args: vec![local(copied)],
                });
            }
        } else {
            b.assign(
                Place::Local(result),
                Rvalue::Call {
                    callee: callee(101 + index, i.int()),
                    args: vec![local(copied)],
                },
            );
        }
        b.terminate(Terminator::Return(Some(local(result))));
        mir.functions.push(b.finish());
    }
    (mir, i)
}

#[test]
fn callback_targets_close_over_long_and_recursive_forwarding() {
    for depth in [1, 32] {
        for cycle in [false, true] {
            let (mut mir, i) = forwarded_callback(depth, cycle);
            assert!(run(&mut mir, &i));
            assert!(
                !mir.functions
                    .iter()
                    .flat_map(|f| &f.blocks)
                    .flat_map(|b| &b.stmts)
                    .any(|s| matches!(s, Statement::Assign(_, Rvalue::IndirectCall { .. })))
            );
            assert!(!run(&mut mir, &i));
        }
    }
}

#[test]
fn debug_does_not_propagate_callback_parameters_through_helpers() {
    let (mut mir, i) = forwarded_callback(3, false);
    mir.profile = dream_abi::profile::CompileProfile::Debug;
    assert!(!run(&mut mir, &i));
}

#[test]
fn forwarding_rejects_unknown_conflicting_and_unseeded_entries() {
    for entry in [
        "unknown",
        "conflicting",
        "unseeded",
        "unseeded_incoming",
        "reassigned",
    ] {
        let (mut mir, i) = forwarded_callback(12, true);
        let sig = mir.functions[0].local_ty(Local(1));
        match entry {
            "unknown" => {
                let mut b = FunctionBuilder::new("exported", i.int());
                b.set_def(DefId::root(900), vec![]);
                let callback = b.new_param(sig, None);
                b.push(Statement::Call {
                    callee: callee(100, i.int()),
                    args: vec![local(callback)],
                });
                b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
                mir.exports.push((DefId::root(900), "entry".into()));
                mir.functions.push(b.finish());
            }
            "conflicting" => {
                let mut other = module(false, false).0.functions.remove(0);
                other.def = DefId::root(901);
                other.blocks[0].stmts[0] =
                    Statement::Assign(Place::Local(Local(0)), Rvalue::FuncRef(callee(9, i.int())));
                other.blocks[0].stmts[3] = Statement::Assign(
                    Place::Local(Local(3)),
                    Rvalue::Call {
                        callee: callee(100, i.int()),
                        args: vec![local(Local(1))],
                    },
                );
                mir.functions.push(other);
            }
            "unseeded" => {
                mir.functions.remove(0);
            }
            "unseeded_incoming" => {
                let mut b = FunctionBuilder::new("unseeded", i.int());
                b.set_def(DefId::root(950), vec![]);
                let callback = b.new_param(sig, None);
                for target in [950, 100] {
                    b.push(Statement::Call {
                        callee: callee(target, i.int()),
                        args: vec![local(callback)],
                    });
                }
                b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
                mir.functions.push(b.finish());
            }
            "reassigned" => mir.functions[2].blocks[0].stmts.push(Statement::Assign(
                Place::Local(Local(0)),
                Rvalue::Use(Operand::Const(Const::Null)),
            )),
            _ => unreachable!(),
        }
        assert!(!run(&mut mir, &i), "{entry}");
    }
}

#[test]
fn passing_a_box_by_reference_invalidates_its_target() {
    let (mut mir, i) = module(false, false);
    let ty = mir.functions[0].local_ty(Local(1));
    let mut b = FunctionBuilder::new("replace", i.void());
    b.set_def(DefId::root(5), vec![]);
    let p = b.new_ref_param(ty, None);
    b.assign(Place::Local(p), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::Return(None));
    mir.functions.push(b.finish());
    mir.functions[0].blocks[0].stmts.insert(
        2,
        Statement::Call {
            callee: callee(5, i.void()),
            args: vec![local(Local(1))],
        },
    );
    assert!(!run(&mut mir, &i));
}

fn with_field() -> (Mir, TypeInterner) {
    let (mut mir, mut i) = module(false, false);
    let ty = i.struct_ty(DefId::root(10), vec![]);
    let func = i.func(vec![i.int()], i.int());
    let void = i.void();
    let mut ctor = FunctionBuilder::new("ctor", void);
    ctor.set_def(DefId::root(11), vec![]);
    let this = ctor.new_param(ty, None);
    let raw = ctor.new_temp(i.int());
    ctor.assign(Place::Local(raw), Rvalue::FuncRef(callee(1, i.int())));
    let boxed = ctor.new_temp(func);
    ctor.assign(
        Place::Local(boxed),
        Rvalue::Call {
            callee: callee(2, func),
            args: vec![local(raw), Operand::Const(Const::Int(0))],
        },
    );
    ctor.assign(
        Place::Field {
            base: this,
            field: 0,
        },
        Rvalue::Use(local(boxed)),
    );
    ctor.terminate(Terminator::Return(None));
    let mut caller = FunctionBuilder::new("field_caller", i.int());
    let obj = caller.new_temp(ty);
    caller.assign(
        Place::Local(obj),
        Rvalue::New {
            def: DefId::root(10),
            ty,
            ctor: Some(crate::NewCtor {
                def: DefId::root(11),
                field_init: false,
                take_params: vec![],
            }),
            args: vec![],
        },
    );
    let callback = caller.new_temp(func);
    caller.assign(
        Place::Local(callback),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: obj,
            field: 0,
        })),
    );
    let target = caller.new_temp(i.int());
    caller.assign(
        Place::Local(target),
        Rvalue::Call {
            callee: callee(3, i.int()),
            args: vec![local(callback)],
        },
    );
    let result = caller.new_temp(i.int());
    caller.assign(
        Place::Local(result),
        Rvalue::IndirectCall {
            target: local(target),
            args: vec![Operand::Const(Const::Int(42))],
            sig: func,
        },
    );
    caller.terminate(Terminator::Return(Some(local(result))));
    mir.functions = vec![ctor.finish(), caller.finish()];
    (mir, i)
}

#[test]
fn initialized_function_field_resolves() {
    let (mut mir, i) = with_field();
    assert!(run(&mut mir, &i));
    assert!(matches!(
        mir.functions[1].blocks[0].stmts[3],
        Statement::Assign(_, Rvalue::Call { .. })
    ));
}

#[test]
fn scheduler_control_preserves_box_targets_but_foreign_calls_do_not() {
    for key in ["dream_cancel", "dream_start", "foreign_unknown"] {
        let (mut mir, i) = with_field();
        let mut b = FunctionBuilder::new("control", i.void());
        b.set_def(DefId::root(91), vec![]);
        let future = b.new_param(i.object(), None);
        b.push(Statement::Call {
            callee: callee(90, i.void()),
            args: vec![local(future)],
        });
        b.terminate(Terminator::Return(None));
        mir.functions.push(b.finish());
        if key != "foreign_unknown" {
            mir.intrinsics.push((DefId::root(90), key.into()));
        }
        assert_eq!(run(&mut mir, &i), key != "foreign_unknown", "{key}");
    }
}

#[test]
fn conflicting_field_write_and_exports_reject_field_invariant() {
    for export in [false, true] {
        let (mut mir, mut i) = with_field();
        if export {
            mir.exports.push((DefId::root(100), "entry".into()));
        } else {
            let ty = mir.functions[1].local_ty(Local(0));
            let mut setter = FunctionBuilder::new("setter", i.void());
            let obj = setter.new_param(ty, None);
            let callback = setter.new_param(i.func(vec![i.int()], i.int()), None);
            setter.assign(
                Place::Field {
                    base: obj,
                    field: 0,
                },
                Rvalue::Use(local(callback)),
            );
            setter.terminate(Terminator::Return(None));
            mir.functions.push(setter.finish());
        }
        assert!(!run(&mut mir, &i));
    }
}

#[test]
fn opaque_call_and_representation_cast_keep_box_dynamic() {
    for cast in [false, true] {
        let (mut mir, i) = module(false, false);
        let boxed = Local(1);
        let stmt = if cast {
            let dest = Local(mir.functions[0].locals.len() as u32);
            let mut decl = mir.functions[0].locals[0].clone();
            decl.ty = i.object();
            mir.functions[0].locals.push(decl);
            Statement::Assign(
                Place::Local(dest),
                Rvalue::Cast(local(boxed), mir.functions[0].local_ty(boxed), i.object()),
            )
        } else {
            Statement::Call {
                callee: callee(90, i.void()),
                args: vec![local(boxed)],
            }
        };
        mir.functions[0].blocks[0].stmts.insert(2, stmt);
        assert!(!run(&mut mir, &i));
    }
}

#[test]
fn capture_free_function_shapes_exclude_captures_foreign_results_and_raw_writes() {
    use crate::analysis::effects::ModRefTable;
    use dream_hir::TypeLayout;
    for mode in 0..4 {
        let (mut mir, mut i) = module(mode == 1, false);
        mir.profile = dream_abi::profile::CompileProfile::Release;
        let func = mir.functions[0].locals[1].ty;
        if mode == 2 {
            mir.functions[0].blocks[0].stmts.push(Statement::RawStore {
                address: Operand::Const(Const::Int(0)),
                value: Operand::Const(Const::Int(0)),
                ty: i.int(),
            });
        } else if mode == 3 {
            let dest = Local(mir.functions[0].locals.len() as u32);
            let mut decl = mir.functions[0].locals[1].clone();
            decl.ty = func;
            mir.functions[0].locals.push(decl);
            mir.functions[0].blocks[0].stmts.push(Statement::Assign(
                Place::Local(dest),
                Rvalue::Call {
                    callee: callee(999, func),
                    args: vec![],
                },
            ));
        }
        let marker = i.struct_ty(DefId::root(741), vec![]);
        let mut layout = TypeLayout::from_fields(&i, "Destructor", []);
        layout.destructor = Some(DefId::root(742));
        mir.layouts.insert(marker, layout);
        let facts = crate::analysis::function_boxes::capture_free_types(&mir, &i);
        assert_eq!(facts.contains(&func), mode == 0);
        let effects = ModRefTable::compute(&mir, &i);
        assert_eq!(effects.may_run_del(func, &i, &mir.layouts), mode != 0);
    }
}
