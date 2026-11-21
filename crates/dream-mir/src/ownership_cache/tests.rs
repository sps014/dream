use super::*;
use crate::build::FunctionBuilder;
use crate::passes::rc::RcInsertion;
use crate::{Callee, Const, Global, MirGlobal, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::{InterfaceImpl, InterfaceInfo, TypeLayout};
use dream_types::{DefKind, TypeCtx};

#[derive(Clone, Copy, PartialEq)]
enum Variant {
    Base,
    WritingImplementer,
    Destructor,
    WeakField,
}

struct Program {
    ctx: TypeCtx,
    mir: Mir,
}

/// `user` keeps a field load across an interface call, so its insertion asks what the
/// interface's implementations may write and whether releasing one may run a destructor. Only
/// the interface type is named in `user`, so implementation changes reach it only through those
/// answers. A shifted program allocates an unrelated declaration first, so every raw id differs
/// from the unshifted one.
fn program(variant: Variant, shifted: bool) -> Program {
    let mut ctx = TypeCtx::new();
    if shifted {
        ctx.register(DefKind::Struct, "Unrelated", vec![]);
    }
    let boxed_def = ctx.register(DefKind::Struct, "Box", vec![]);
    let implementer_def = ctx.register(DefKind::Struct, "Impl", vec![]);
    let writer_def = ctx.register(DefKind::Interface, "Writer", vec![]);
    let keep_def = ctx.register(DefKind::Function, "keep", vec![]);
    let user_def = ctx.register(DefKind::Function, "user", vec![]);
    let method_def = ctx.register(DefKind::Function, "method", vec![]);
    let del_def = ctx.register(DefKind::Function, "implementer_del", vec![]);
    let i = &mut ctx.interner;
    let boxed = i.struct_ty(boxed_def, vec![]);
    let implementer = i.struct_ty(implementer_def, vec![]);
    let writer = i.interface_ty(writer_def, vec![]);
    let void = i.void();
    let sig = i.func(vec![writer], void);

    let clear_global = |b: &mut FunctionBuilder| {
        let target = b.new_local(boxed, None);
        b.assign(
            Place::Local(target),
            Rvalue::Use(Operand::Copy(Place::Global(Global(0)))),
        );
        b.assign(
            Place::Field {
                base: target,
                field: 0,
            },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    };

    let mut keep = FunctionBuilder::new("keep", void);
    keep.set_def(keep_def, vec![]);
    keep.new_param(boxed, None);
    keep.terminate(Terminator::Return(None));

    let mut method = FunctionBuilder::new("method", void);
    method.set_def(method_def, vec![]);
    method.new_param(implementer, None);
    if variant == Variant::WritingImplementer {
        clear_global(&mut method);
    }
    method.terminate(Terminator::Return(None));

    let mut user = FunctionBuilder::new("user", void);
    user.set_def(user_def, vec![]);
    let base = user.new_param(boxed, None);
    let receiver = user.new_take_param(writer, None);
    let field = user.new_local(boxed, None);
    user.assign(
        Place::Local(field),
        Rvalue::Use(Operand::Copy(Place::Field { base, field: 0 })),
    );
    user.push(Statement::InterfaceCall {
        receiver: Operand::Copy(Place::Local(receiver)),
        iface_id: 0,
        method_slot: 0,
        sig,
        args: vec![],
    });
    user.push(Statement::Call {
        callee: Callee {
            borrowed_result: false,
            def: keep_def,
            args: vec![],
            ret: void,
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(field))],
    });
    user.terminate(Terminator::Return(None));

    let mut functions = vec![keep.finish(), method.finish(), user.finish()];
    let mut mir = Mir::default();
    if variant == Variant::Destructor {
        let mut del = FunctionBuilder::new("implementer_del", void);
        del.set_def(del_def, vec![]);
        del.new_param(implementer, None);
        clear_global(&mut del);
        del.terminate(Terminator::Return(None));
        functions.push(del.finish());
    }
    mir.functions = functions;
    mir.globals.push(MirGlobal {
        id: Global(0),
        name: "g".into(),
        ty: boxed,
    });
    let weak = variant == Variant::WeakField;
    mir.layouts.insert(
        boxed,
        TypeLayout::from_fields(i, "Box", [("f".to_string(), boxed, weak, false)]),
    );
    let mut implementer_layout = TypeLayout::from_fields(i, "Impl", []);
    if variant == Variant::Destructor {
        implementer_layout.destructor = Some(del_def);
    }
    mir.layouts.insert(implementer, implementer_layout);
    mir.interfaces.interfaces.push(InterfaceInfo {
        ty: writer,
        name: "Writer".into(),
        method_count: 1,
        sigs: vec![sig],
    });
    mir.interfaces.impls.push(InterfaceImpl {
        class_ty: implementer,
        entries: vec![(0, vec![Some(method_def)])],
    });
    Program { ctx, mir }
}

fn prepare(cache: &mut OwnershipCache, program: &mut Program) {
    let interner = &program.ctx.interner;
    let mir = &mut program.mir;
    let layouts = mir.layouts.clone();
    let holds = IndexSet::new();
    let mut session = cache.begin(mir, interner).expect("every id has a stable identity");
    let modref = cache.modref(&mut session, mir, interner);
    cache.insert(session, mir, (interner, &layouts, &modref), &holds, |f| {
        RcInsertion::run_with_layouts(f, interner, &layouts, &holds, &modref);
    });
}

fn fresh(variant: Variant) -> Vec<String> {
    let mut program = program(variant, true);
    let layouts = program.mir.layouts.clone();
    let modref = ModRefTable::compute(&program.mir, &program.ctx.interner);
    for f in &mut program.mir.functions {
        RcInsertion::run_with_layouts(
            f,
            &program.ctx.interner,
            &layouts,
            &IndexSet::new(),
            &modref,
        );
    }
    bodies(&program.mir)
}

fn bodies(mir: &Mir) -> Vec<String> {
    mir.functions.iter().map(|f| format!("{f:?}")).collect()
}

/// Stores `stored`'s results, then prepares the shifted `variant` against them. Returns the
/// cache and the names of the functions whose inserted body was taken from it.
fn rebuild(stored: Variant, variant: Variant) -> (OwnershipCache, Vec<String>, Vec<String>) {
    let mut first = program(stored, false);
    let mut cache = OwnershipCache::new(&first.ctx.defs);
    prepare(&mut cache, &mut first);
    assert_eq!(cache.reused(), 0);
    let changed = cache.changed();
    assert_eq!(changed.len(), 1);

    let mut second = program(variant, true);
    let mut next = OwnershipCache::new(&second.ctx.defs);
    for (module, bytes) in &changed {
        next.load(module, bytes);
    }
    prepare(&mut next, &mut second);
    let reused = second
        .mir
        .functions
        .iter()
        .filter(|f| {
            let key = ("".to_string(), f.symbol.clone());
            let now = next.current.get(&key.0).and_then(|e| e.get(&key.1));
            let before = next.previous(&key);
            now.and_then(|e| e.inserted.as_ref()).is_some()
                && now.map(|e| &e.inserted) == before.map(|e| &e.inserted)
        })
        .map(|f| f.name.clone())
        .collect();
    let bodies = bodies(&second.mir);
    (next, reused, bodies)
}

#[test]
fn unchanged_functions_reuse_their_inserted_bodies_across_renumbered_ids() {
    let (cache, reused, bodies) = rebuild(Variant::Base, Variant::Base);
    assert_eq!(reused, ["keep", "method", "user"]);
    assert_eq!(cache.reused(), 3);
    assert_eq!(bodies, fresh(Variant::Base));
    assert!(cache.changed().is_empty());
}

#[test]
fn a_new_interface_implementation_effect_invalidates_its_callers() {
    let (_, reused, bodies) = rebuild(Variant::Base, Variant::WritingImplementer);
    assert_eq!(reused, ["keep"]);
    assert_eq!(bodies, fresh(Variant::WritingImplementer));
}

#[test]
fn a_new_destructor_invalidates_functions_that_asked_what_releases_run() {
    let (_, reused, bodies) = rebuild(Variant::Base, Variant::Destructor);
    assert!(reused.contains(&"keep".to_string()), "{reused:?}");
    assert!(!reused.contains(&"user".to_string()), "{reused:?}");
    assert_eq!(bodies, fresh(Variant::Destructor));
}

#[test]
fn a_layout_change_invalidates_functions_that_name_the_type() {
    let (_, reused, bodies) = rebuild(Variant::Base, Variant::WeakField);
    assert!(!reused.contains(&"user".to_string()), "{reused:?}");
    assert!(!reused.contains(&"keep".to_string()), "{reused:?}");
    assert_eq!(bodies, fresh(Variant::WeakField));
}

#[test]
fn local_summaries_are_reused_for_unchanged_functions() {
    let mut first = program(Variant::Base, false);
    let mut cache = OwnershipCache::new(&first.ctx.defs);
    prepare(&mut cache, &mut first);
    let changed = cache.changed();
    let second = program(Variant::Base, true);
    let mut next = OwnershipCache::new(&second.ctx.defs);
    for (module, bytes) in &changed {
        next.load(module, bytes);
    }
    let interner = &second.ctx.interner;
    let mut session = next.begin(&second.mir, interner).unwrap();
    let hits = std::cell::Cell::new(0);
    let cached = |index: usize, may_destroy: bool| {
        let key = session.summary_key(index, may_destroy)?;
        let (stored, _) = next
            .previous(session.keys[index].as_ref()?)?
            .summary
            .clone()?;
        (stored == key).then(|| hits.set(hits.get() + 1))
    };
    for index in 0..second.mir.functions.len() {
        cached(index, false);
        cached(index, true);
    }
    assert_eq!(hits.get(), second.mir.functions.len());
    let table = next.modref(&mut session, &second.mir, interner);
    let expected = ModRefTable::compute(&second.mir, interner);
    for f in &second.mir.functions {
        assert_eq!(
            table.call_def(f.def, &f.instance),
            expected.call_def(f.def, &f.instance)
        );
    }
}
