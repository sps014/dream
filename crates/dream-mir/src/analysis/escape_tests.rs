//! Escape levels: reads stay `No`, calls into keeping-nothing parameters are `Arg`, and a store,
//! return or escaping callee parameter anywhere in the alias class makes it `Global`.

use super::escape::{Escape, LocalEscape, ParamSummaries};
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId, TypeInterner};

const NODE: DefId = DefId(7);
const READ: DefId = DefId(40);
const KEEP: DefId = DefId(41);
const RELAY: DefId = DefId(42);

fn node(i: &mut TypeInterner) -> TypeId {
    i.struct_ty(NODE, vec![])
}

fn new_node(ty: TypeId) -> Rvalue {
    Rvalue::New {
        def: NODE,
        ty,
        ctor: None,
        args: vec![],
    }
}

fn copy(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn call(def: DefId, i: &TypeInterner, arg: Local) -> Statement {
    Statement::Call {
        callee: Callee {
            def,
            args: vec![],
            ret: i.void(),
            take_params: vec![false],
        },
        args: vec![copy(arg)],
    }
}

/// `read(n)` loads a field; `keep(n)` stores `n` into a global; `relay(n)` forwards to `keep`.
fn callees(i: &mut TypeInterner) -> Vec<MirFunction> {
    let ty = node(i);
    let mut read = FunctionBuilder::new("read", i.int());
    read.set_def(READ, vec![]);
    let n = read.new_param(ty, Some("n".into()));
    let v = read.new_temp(i.int());
    read.assign(
        Place::Local(v),
        Rvalue::Use(Operand::Copy(Place::Field { base: n, field: 0 })),
    );
    read.terminate(Terminator::Return(Some(copy(v))));

    let mut keep = FunctionBuilder::new("keep", i.void());
    keep.set_def(KEEP, vec![]);
    let n = keep.new_param(ty, Some("n".into()));
    keep.assign(Place::Global(crate::Global(0)), Rvalue::Use(copy(n)));
    keep.terminate(Terminator::Return(None));

    let mut relay = FunctionBuilder::new("relay", i.void());
    relay.set_def(RELAY, vec![]);
    let n = relay.new_param(ty, Some("n".into()));
    let alias = relay.new_temp(ty);
    relay.assign(Place::Local(alias), Rvalue::Use(copy(n)));
    relay.push(call(KEEP, i, alias));
    relay.terminate(Terminator::Return(None));

    vec![read.finish(), keep.finish(), relay.finish()]
}

/// `o = new Node; a = o; <tail(a)>; return`, analysed against `callees`.
fn level_of(tail: impl FnOnce(&mut FunctionBuilder, &TypeInterner, Local, TypeId)) -> (Escape, bool) {
    let mut i = TypeInterner::new();
    let ty = node(&mut i);
    let mut mir = Mir {
        functions: callees(&mut i),
        ..Mir::default()
    };
    let mut b = FunctionBuilder::new("f", i.void());
    let o = b.new_local(ty, Some("o".into()));
    let a = b.new_temp(ty);
    b.assign(Place::Local(o), new_node(ty));
    b.assign(Place::Local(a), Rvalue::Use(copy(o)));
    b.push(Statement::Retain(copy(a)));
    tail(&mut b, &i, a, ty);
    b.push(Statement::Release(copy(o)));
    mir.functions.push(b.finish());
    let sums = ParamSummaries::compute(&mir, &i);
    let f = mir.functions.last().expect("f");
    let esc = LocalEscape::analyze(f, &i, &sums);
    (esc.of(o), esc.same_class(o, a))
}

#[test]
fn field_reads_and_rc_do_not_escape() {
    let (level, joined) = level_of(|b, i, a, _| {
        let v = b.new_temp(i.int());
        b.assign(
            Place::Local(v),
            Rvalue::Use(Operand::Copy(Place::Field { base: a, field: 0 })),
        );
        b.assign(
            Place::Field { base: a, field: 0 },
            Rvalue::Use(Operand::Const(Const::Int(1))),
        );
        b.terminate(Terminator::Return(None));
    });
    assert_eq!(level, Escape::No);
    assert!(joined, "a copy joins its source's class");
}

#[test]
fn a_reading_callee_is_an_argument_escape() {
    let (level, _) = level_of(|b, i, a, _| {
        b.push(call(READ, i, a));
        b.terminate(Terminator::Return(None));
    });
    assert_eq!(level, Escape::Arg);
}

#[test]
fn a_keeping_callee_escapes_through_its_relay() {
    let (level, _) = level_of(|b, i, a, _| {
        b.push(call(RELAY, i, a));
        b.terminate(Terminator::Return(None));
    });
    assert_eq!(level, Escape::Global);
}

#[test]
fn an_unknown_callee_keeps_its_argument() {
    let (level, _) = level_of(|b, i, a, _| {
        b.push(call(DefId(99), i, a));
        b.terminate(Terminator::Return(None));
    });
    assert_eq!(level, Escape::Global);
}

#[test]
fn stores_and_returns_of_an_alias_escape() {
    let (stored, _) = level_of(|b, _, a, ty| {
        let other = b.new_temp(ty);
        b.assign(Place::Field { base: other, field: 1 }, Rvalue::Use(copy(a)));
        b.terminate(Terminator::Return(None));
    });
    assert_eq!(stored, Escape::Global);
    let (returned, _) = level_of(|b, _, a, _| {
        b.terminate(Terminator::Return(Some(copy(a))));
    });
    assert_eq!(returned, Escape::Global);
}

#[test]
fn recursion_through_a_reading_cycle_stays_optimistic() {
    // `even(n) { odd(n) }`, `odd(n) { read(n); even(n) }`: nothing on the cycle keeps `n`.
    let mut i = TypeInterner::new();
    let ty = node(&mut i);
    let mut fns = callees(&mut i);
    for (name, def, next) in [("even", DefId(50), DefId(51)), ("odd", DefId(51), DefId(50))] {
        let mut b = FunctionBuilder::new(name, i.void());
        b.set_def(def, vec![]);
        let n = b.new_param(ty, Some("n".into()));
        b.push(call(READ, &i, n));
        b.push(call(next, &i, n));
        b.terminate(Terminator::Return(None));
        fns.push(b.finish());
    }
    let mir = Mir {
        functions: fns,
        ..Mir::default()
    };
    let sums = ParamSummaries::compute(&mir, &i);
    assert!(!sums.param_escapes(DefId(50), &[], 0));
    assert!(!sums.param_escapes(DefId(51), &[], 0));
    assert!(sums.param_escapes(KEEP, &[], 0));
    assert!(sums.param_escapes(RELAY, &[], 0));
}
