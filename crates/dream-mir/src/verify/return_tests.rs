use super::*;
use crate::build::FunctionBuilder;
use crate::{Callee, Local, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, DefKind, TypeCtx};

fn copy(local: Local) -> Operand {
    Operand::Copy(Place::Local(local))
}

fn call(def: DefId, ret: dream_types::TypeId, args: Vec<Operand>) -> Rvalue {
    Rvalue::Call {
        callee: Callee {
            def,
            ret,
            args: vec![],
            take_params: vec![],
        },
        args,
    }
}

#[test]
fn factories_cannot_hide_region_allocations_but_identity_preserves_older_objects() {
    for identity in [false, true] {
        let mut ctx = TypeCtx::new();
        let node = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(node, vec![]);
        let def = ctx.register(DefKind::Function, "callee", vec![]);
        let mut callee = FunctionBuilder::new("callee", ty);
        callee.set_def(def, vec![]);
        let param = callee.new_param(ty, None);
        let value = callee.new_local(ty, None);
        callee.assign(
            Place::Local(value),
            if identity {
                Rvalue::Use(copy(param))
            } else {
                Rvalue::New {
                    def: node,
                    ty,
                    ctor: None,
                    args: vec![],
                }
            },
        );
        callee.terminate(Terminator::Return(Some(copy(value))));

        let mut caller = FunctionBuilder::new("caller", ty);
        caller.set_def(DefId::root(def.index + 1), vec![]);
        let old = caller.new_param(ty, None);
        let result = caller.new_local(ty, None);
        caller.push(Statement::RegionEnter);
        caller.assign(Place::Local(result), call(def, ty, vec![copy(old)]));
        caller.push(Statement::RegionLeave);
        caller.terminate(Terminator::Return(Some(copy(result))));
        let mir = Mir {
            functions: vec![caller.finish(), callee.finish()],
            ..Mir::default()
        };
        let found = verify_module(&mir, &ctx.interner);
        assert_eq!(found.is_empty(), identity, "{found:?}");
        if !identity {
            assert!(found[0].msg.contains("allocation region was left"));
        }
    }
}

#[test]
fn an_opaque_call_cannot_erase_its_argument_origin() {
    let mut ctx = TypeCtx::new();
    let node = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node, vec![]);
    let mut f = FunctionBuilder::new("opaque", ty);
    let input = f.new_local(ty, None);
    let output = f.new_local(ty, None);
    f.push(Statement::RegionEnter);
    f.assign(
        Place::Local(input),
        Rvalue::New {
            def: node,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    f.assign(
        Place::Local(output),
        call(DefId::root(100), ty, vec![copy(input)]),
    );
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(Some(copy(output))));
    let found = verify_function(&f.finish(), &ctx.interner);
    assert!(found
        .iter()
        .any(|v| v.msg.contains("allocation region was left")));
    assert!(found
        .iter()
        .any(|v| v.msg.contains("escape through a call")));
}

#[test]
fn call_side_effects_preserve_region_children_inserted_into_an_older_root() {
    let mut ctx = TypeCtx::new();
    let node = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node, vec![]);
    let def = ctx.register(DefKind::Function, "install", vec![]);
    let mut install = FunctionBuilder::new("install", ctx.interner.void());
    install.set_def(def, vec![]);
    let target = install.new_param(ty, None);
    let child = install.new_local(ty, None);
    install.assign(
        Place::Local(child),
        Rvalue::New {
            def: node,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    install.assign(
        Place::Field {
            base: target,
            field: 0,
        },
        Rvalue::Use(copy(child)),
    );
    install.terminate(Terminator::Return(None));
    let mut caller = FunctionBuilder::new("caller", ty);
    caller.set_def(DefId::root(def.index + 1), vec![]);
    let root = caller.new_param(ty, None);
    caller.push(Statement::RegionEnter);
    caller.push(Statement::Call {
        callee: Callee {
            def,
            ret: ctx.interner.void(),
            args: vec![],
            take_params: vec![],
        },
        args: vec![copy(root)],
    });
    caller.push(Statement::RegionLeave);
    caller.terminate(Terminator::Return(Some(copy(root))));
    let mir = Mir {
        functions: vec![caller.finish(), install.finish()],
        ..Mir::default()
    };
    let found = verify_module(&mir, &ctx.interner);
    assert!(found.iter().all(|v| v.func == "caller"));
    assert!(found
        .iter()
        .any(|v| v.msg.contains("allocation region was left")));
}

#[test]
fn mutually_recursive_return_summaries_reach_a_fixed_point() {
    let mut ctx = TypeCtx::new();
    let node = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(node, vec![]);
    let mut functions = Vec::new();
    for def in [DefId::root(10), DefId::root(11)] {
        let mut f = FunctionBuilder::new("recursive", ty);
        f.set_def(def, vec![]);
        let param = f.new_param(ty, None);
        let local = f.new_local(ty, None);
        f.assign(
            Place::Local(local),
            call(DefId::root(21 - def.index), ty, vec![copy(param)]),
        );
        if def == DefId::root(11) {
            f.assign(Place::Local(local), Rvalue::Use(copy(param)));
        }
        f.terminate(Terminator::Return(Some(copy(local))));
        functions.push(f.finish());
    }
    let mir = Mir {
        functions,
        ..Mir::default()
    };
    let summaries = returns::summarize(&mir, &ctx.interner);
    for def in [DefId::root(10), DefId::root(11)] {
        let result = &summaries[&(def, vec![])].result;
        assert_eq!(result.params, std::collections::BTreeSet::from([0]));
        assert!(!result.fresh);
    }
}
