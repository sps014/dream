use super::param_modes;
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Global, Mir, Operand, Place, Rvalue, Statement, Terminator};
use dream_abi::profile::CompileProfile;
use dream_types::{DefId, TypeInterner};

fn recursive(i: &mut TypeInterner, managed: bool) -> crate::MirFunction {
    let elem = if managed { i.string() } else { i.int() };
    let ty = i.array(elem);
    let mut b = FunctionBuilder::new("walk", i.void());
    b.set_def(DefId::root(1), vec![]);
    let array = b.new_take_param(ty, None);
    b.push(Statement::SourceLine(1));
    b.push(Statement::Call {
        callee: Callee {
            def: DefId::root(1),
            args: vec![],
            ret: i.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(array))],
    });
    b.terminate(Terminator::Return(None));
    b.finish()
}

fn module(f: crate::MirFunction) -> Mir {
    Mir {
        profile: CompileProfile::Release,
        functions: vec![f],
        ..Mir::default()
    }
}

#[test]
fn recursive_scalar_array_kernel_borrows_and_rewrites_callers() {
    let mut i = TypeInterner::new();
    let mut f = recursive(&mut i, false);
    f.blocks[0].stmts.insert(
        0,
        Statement::Assign(
            Place::index(f.params[0], Operand::Const(Const::Int(0))),
            Rvalue::Use(Operand::Const(Const::Int(42))),
        ),
    );
    let mut mir = module(f);
    let mut caller = FunctionBuilder::new("caller", i.void());
    caller.set_def(DefId::root(2), vec![]);
    let array = caller.new_take_param(mir.functions[0].locals[0].ty, None);
    caller.push(Statement::Call {
        callee: Callee {
            def: DefId::root(1),
            args: vec![],
            ret: i.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(array))],
    });
    caller.terminate(Terminator::Return(None));
    mir.functions.push(caller.finish());
    assert!(param_modes::run(&mut mir, &i));
    assert!(!mir.functions[0].locals[0].is_take);
    for f in &mir.functions {
        for s in &f.blocks[0].stmts {
            if let Statement::Call { callee, .. } = s {
                assert_eq!(callee.take_params, vec![false]);
            }
        }
    }
    assert!(!param_modes::run(&mut mir, &i));
}

#[test]
fn opaque_calls_and_owner_mutation_prevent_borrowing() {
    let mut i = TypeInterner::new();
    for global_store in [false, true] {
        let mut f = recursive(&mut i, false);
        if global_store {
            f.blocks[0].stmts.insert(
                0,
                Statement::Assign(
                    Place::Global(Global(0)),
                    Rvalue::Use(Operand::Const(Const::Null)),
                ),
            );
        } else if let Statement::Call { callee, .. } = &mut f.blocks[0].stmts[1] {
            callee.def = DefId::root(999);
        }
        let mut mir = module(f);
        assert!(!param_modes::run(&mut mir, &i));
        assert!(mir.functions[0].locals[0].is_take);
    }
}

#[test]
fn async_exports_and_debug_keep_ownership() {
    let mut i = TypeInterner::new();
    for exclusion in 1..5 {
        let mut f = recursive(&mut i, false);
        f.is_async = exclusion == 1;
        let mut mir = module(f);
        if exclusion == 2 {
            mir.exports.push((DefId::root(1), "walk".into()));
        }
        if exclusion == 3 {
            mir.profile = CompileProfile::Debug;
        }
        if exclusion == 4 {
            mir.functions[0].blocks[0]
                .stmts
                .push(Statement::DebugLine(1));
        }
        assert!(!param_modes::run(&mut mir, &i));
        assert!(mir.functions[0].locals[0].is_take);
    }
}

#[test]
fn unsafe_callee_rejection_propagates_through_recursive_component() {
    let mut i = TypeInterner::new();
    let a = recursive(&mut i, false);
    let mut b = recursive(&mut i, false);
    b.def = DefId::root(2);
    if let Statement::Call { callee, .. } = &mut b.blocks[0].stmts[1] {
        callee.def = DefId::root(999);
    }
    let mut mir = module(a);
    let edge = Statement::Call {
        callee: Callee {
            def: b.def,
            args: vec![],
            ret: i.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(mir.functions[0].params[0]))],
    };
    mir.functions[0].blocks[0].stmts.push(edge);
    mir.functions.push(b);
    assert!(!param_modes::run(&mut mir, &i));
    assert!(mir.functions.iter().all(|f| f.locals[0].is_take));
}

#[test]
fn string_reader_borrows_but_escaping_and_opaque_helpers_do_not() {
    let i = TypeInterner::new();
    for unsafe_use in 0..4 {
        let mut b = FunctionBuilder::new("length", i.int());
        b.set_def(DefId::root(10), vec![]);
        let s = b.new_take_param(i.string(), None);
        let n = b.new_local(i.int(), None);
        b.assign(
            Place::Local(n),
            Rvalue::StrLen(Operand::Copy(Place::Local(s))),
        );
        match unsafe_use {
            1 => b.assign(
                Place::Global(Global(0)),
                Rvalue::Use(Operand::Copy(Place::Local(s))),
            ),
            2 => b.push(Statement::Call {
                callee: Callee {
                    def: DefId::root(999),
                    args: vec![],
                    ret: i.void(),
                    take_params: vec![false],
                },
                args: vec![Operand::Copy(Place::Local(s))],
            }),
            _ => {}
        }
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(n)))));
        let mut mir = module(b.finish());
        if unsafe_use == 3 {
            mir.exports.push((DefId::root(10), "length".into()));
        }
        assert_eq!(param_modes::run(&mut mir, &i), unsafe_use == 0);
        assert_eq!(
            mir.functions[0].locals[s.0 as usize].is_take,
            unsafe_use != 0
        );
    }
}

#[test]
fn managed_array_reader_and_recursive_forwarding_borrow() {
    let mut i = TypeInterner::new();
    let mut mir = module(recursive(&mut i, true));
    assert!(param_modes::run(&mut mir, &i));
    assert!(!mir.functions[0].locals[0].is_take);
}

#[test]
fn returned_alias_does_not_block_unrelated_parameter() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("choose", i.string());
    b.set_def(DefId::root(20), vec![]);
    let read = b.new_take_param(i.string(), None);
    let returned = b.new_take_param(i.string(), None);
    let alias = b.new_local(i.string(), None);
    let length = b.new_local(i.int(), None);
    b.assign(
        Place::Local(length),
        Rvalue::StrLen(Operand::Copy(Place::Local(read))),
    );
    b.assign(
        Place::Local(alias),
        Rvalue::Use(Operand::Copy(Place::Local(returned))),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(alias)))));
    let mut mir = module(b.finish());
    assert!(param_modes::run(&mut mir, &i));
    assert!(!mir.functions[0].locals[read.0 as usize].is_take);
    assert!(mir.functions[0].locals[returned.0 as usize].is_take);
    assert!(!param_modes::run(&mut mir, &i));
}

#[test]
fn sink_dependency_rejects_only_the_forwarded_parameter() {
    let i = TypeInterner::new();
    let mut sink = FunctionBuilder::new("identity", i.string());
    sink.set_def(DefId::root(21), vec![]);
    let s = sink.new_take_param(i.string(), None);
    sink.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
    let mut caller = FunctionBuilder::new("reader", i.void());
    caller.set_def(DefId::root(22), vec![]);
    let read = caller.new_take_param(i.string(), None);
    let forwarded = caller.new_take_param(i.string(), None);
    let alias = caller.new_local(i.string(), None);
    let n = caller.new_local(i.int(), None);
    caller.assign(
        Place::Local(n),
        Rvalue::StrLen(Operand::Copy(Place::Local(read))),
    );
    caller.assign(
        Place::Local(alias),
        Rvalue::Call {
            callee: Callee {
                def: DefId::root(21),
                args: vec![],
                ret: i.string(),
                take_params: vec![true],
            },
            args: vec![Operand::Copy(Place::Local(forwarded))],
        },
    );
    caller.terminate(Terminator::Return(None));
    let mut mir = module(caller.finish());
    mir.functions.push(sink.finish());
    assert!(param_modes::run(&mut mir, &i));
    assert!(!mir.functions[0].locals[read.0 as usize].is_take);
    assert!(mir.functions[0].locals[forwarded.0 as usize].is_take);
    assert!(mir.functions[1].locals[s.0 as usize].is_take);
}

#[test]
fn reassigning_parameter_keeps_owning_slot() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("replace", i.void());
    b.set_def(DefId::root(23), vec![]);
    let s = b.new_take_param(i.string(), None);
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(Const::Str("replacement".into()))),
    );
    b.terminate(Terminator::Return(None));
    let mut mir = module(b.finish());
    assert!(!param_modes::run(&mut mir, &i));
    assert!(mir.functions[0].locals[s.0 as usize].is_take);
}

#[test]
fn nested_destructors_and_observer_fields_prevent_borrowing() {
    use dream_hir::TypeLayout;
    use dream_types::TyKind;
    for exclusion in 0..4 {
        let mut i = TypeInterner::new();
        let child = i.intern(TyKind::Struct(DefId::root(30), vec![]));
        let parent = i.intern(TyKind::Struct(DefId::root(31), vec![]));
        let mut child_layout = TypeLayout::from_fields(
            &i,
            "Child",
            [("text".into(), i.string(), exclusion == 1, exclusion == 2)],
        );
        if exclusion == 0 {
            child_layout.destructor = Some(DefId::root(32));
        }
        let parent_layout =
            TypeLayout::from_fields(&i, "Parent", [("child".into(), child, false, false)]);
        let mut b = FunctionBuilder::new("read", i.int());
        b.set_def(DefId::root(33), vec![]);
        let p = b.new_take_param(parent, None);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(1)))));
        let mut mir = module(b.finish());
        mir.layouts.structs.insert(child, child_layout);
        mir.layouts.structs.insert(parent, parent_layout);
        assert_eq!(param_modes::run(&mut mir, &i), exclusion == 3);
        assert_eq!(
            mir.functions[0].locals[p.0 as usize].is_take,
            exclusion != 3
        );
    }
}

#[test]
fn branch_and_loop_carried_aliases_cannot_hide_a_returned_parameter() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("choose", i.string());
    b.set_def(DefId::root(34), vec![]);
    let p = b.new_take_param(i.string(), None);
    let a = b.new_local(i.string(), None);
    let c = b.new_local(i.string(), None);
    let exit = b.new_block();
    let loop_block = b.new_block();
    b.assign(Place::Local(a), Rvalue::Use(Operand::Copy(Place::Local(p))));
    b.terminate(Terminator::Goto(loop_block));
    b.switch_to(loop_block);
    b.assign(Place::Local(c), Rvalue::Use(Operand::Copy(Place::Local(a))));
    b.assign(Place::Local(a), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::If {
        cond: Operand::Const(Const::Bool(false)),
        then_blk: loop_block,
        else_blk: exit,
    });
    b.switch_to(exit);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
    let mut mir = module(b.finish());
    assert!(!param_modes::run(&mut mir, &i));
    assert!(mir.functions[0].locals[p.0 as usize].is_take);
}

#[test]
fn constructor_arguments_keep_the_same_abi_as_new_sites() {
    let i = TypeInterner::new();
    let mut ctor = FunctionBuilder::new("constructor", i.void());
    ctor.set_def(DefId::root(35), vec![]);
    let p = ctor.new_take_param(i.string(), None);
    ctor.terminate(Terminator::Return(None));
    let mut caller = FunctionBuilder::new("caller", i.void());
    caller.set_def(DefId::root(36), vec![]);
    let value = caller.new_local(i.string(), None);
    caller.assign(
        Place::Local(value),
        Rvalue::New {
            def: DefId::root(37),
            ty: i.string(),
            ctor: Some(crate::NewCtor {
                def: DefId::root(35),
                take_params: vec![true],
                field_init: false,
            }),
            args: vec![Operand::Const(Const::Str("value".into()))],
        },
    );
    caller.terminate(Terminator::Return(None));
    let mut mir = module(ctor.finish());
    mir.functions.push(caller.finish());
    assert!(!param_modes::run(&mut mir, &i));
    assert!(mir.functions[0].locals[p.0 as usize].is_take);
}
