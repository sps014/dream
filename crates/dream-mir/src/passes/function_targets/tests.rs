use super::*;
use crate::build::FunctionBuilder;

fn local(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}
fn callee(def: u32, ret: TypeId) -> Callee {
    Callee {
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
fn capture_and_reassignment_stay_indirect() {
    for (capture, reassign) in [(true, false), (false, true)] {
        let (mut mir, i) = module(capture, reassign);
        assert!(!run(&mut mir, &i));
    }
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
