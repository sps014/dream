use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Local, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefKind, TypeCtx, TypeId};

fn copy(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}
fn new(ty: TypeId) -> Rvalue {
    Rvalue::New {
        def: dream_types::DefId::root(0),
        ty,
        ctor: None,
        args: vec![],
    }
}
fn node(ctx: &mut TypeCtx) -> TypeId {
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    ctx.interner.struct_ty(def, vec![])
}

#[test]
fn alias_mutations_are_visible_to_the_original_caller_graph_without_later_reads() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("alias_store", ctx.interner.void());
    let root = f.new_param(ty, None);
    let alias = f.new_local(ty, None);
    let child = f.new_local(ty, None);
    f.assign(Place::Local(alias), Rvalue::Use(copy(root)));
    f.push(Statement::RegionEnter);
    f.assign(Place::Local(child), new(ty));
    f.assign(
        Place::Field {
            base: alias,
            field: 0,
        },
        Rvalue::Use(copy(child)),
    );
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(None));
    assert!(
        verify_function(&f.finish(), &ctx.interner)
            .iter()
            .any(|v| v.msg.contains("caller or global graph"))
    );
}

#[test]
fn globals_cannot_publish_regional_allocations_even_without_later_reads() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("global_store", ctx.interner.void());
    let child = f.new_local(ty, None);
    f.push(Statement::RegionEnter);
    f.assign(Place::Local(child), new(ty));
    f.assign(Place::Global(crate::Global(0)), Rvalue::Use(copy(child)));
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(None));
    assert!(
        verify_function(&f.finish(), &ctx.interner)
            .iter()
            .any(|v| v.msg.contains("caller or global graph"))
    );
}

#[test]
fn inline_tuples_inherit_only_their_reference_elements_not_a_heap_envelope() {
    for regional_child in [false, true] {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let tuple = ctx.interner.tuple_ty(vec![ty]);
        let mut f = FunctionBuilder::new("tuple", tuple);
        let child = f.new_local(ty, None);
        let value = f.new_local(tuple, None);
        if !regional_child {
            f.assign(Place::Local(child), new(ty));
        }
        f.push(Statement::RegionEnter);
        if regional_child {
            f.assign(Place::Local(child), new(ty));
        }
        f.assign(
            Place::Local(value),
            Rvalue::Tuple {
                ty: tuple,
                elems: vec![copy(child)],
            },
        );
        f.push(Statement::RegionLeave);
        f.terminate(Terminator::Return(Some(copy(value))));
        let found = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(found.is_empty(), !regional_child, "{found:?}");
    }
}

#[test]
fn scalar_snapshots_and_redefined_aliases_do_not_inherit_region_death() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let mut f = FunctionBuilder::new("snapshot", ctx.interner.int());
    let child = f.new_local(ty, None);
    let snapshot = f.new_local(ctx.interner.int(), None);
    f.push(Statement::RegionEnter);
    f.assign(Place::Local(child), new(ty));
    f.assign(
        Place::Local(snapshot),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: child,
            field: 0,
        })),
    );
    f.push(Statement::RegionLeave);
    f.assign(
        Place::Local(child),
        Rvalue::Use(Operand::Const(Const::Null)),
    );
    f.terminate(Terminator::Return(Some(copy(snapshot))));
    assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
}

#[test]
fn direct_callee_publication_is_checked_at_the_call_boundary() {
    let mut ctx = TypeCtx::new();
    let ty = node(&mut ctx);
    let def = ctx.register(DefKind::Function, "publish", vec![]);
    let mut publish = FunctionBuilder::new("publish", ctx.interner.void());
    publish.set_def(def, vec![]);
    let param = publish.new_param(ty, None);
    publish.assign(Place::Global(crate::Global(0)), Rvalue::Use(copy(param)));
    publish.terminate(Terminator::Return(None));
    let mut caller = FunctionBuilder::new("caller", ctx.interner.void());
    caller.set_def(dream_types::DefId::root(def.index + 1), vec![]);
    let value = caller.new_local(ty, None);
    caller.push(Statement::RegionEnter);
    caller.assign(Place::Local(value), new(ty));
    caller.push(Statement::Call {
        callee: crate::Callee {
            def,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![],
        },
        args: vec![copy(value)],
    });
    caller.push(Statement::RegionLeave);
    caller.terminate(Terminator::Return(None));
    let mir = Mir {
        functions: vec![caller.finish(), publish.finish()],
        ..Mir::default()
    };
    assert!(
        verify_module(&mir, &ctx.interner)
            .iter()
            .any(|v| v.func == "caller" && v.msg.contains("escape through a call"))
    );
}
