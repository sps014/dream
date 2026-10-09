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
fn managed_arrays_async_exports_and_debug_keep_ownership() {
    let mut i = TypeInterner::new();
    for exclusion in 0..5 {
        let mut f = recursive(&mut i, exclusion == 0);
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
