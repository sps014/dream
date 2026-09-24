//! Post-inline held-by-owner tests: `x = base.s; Retain(x); …; Release(x)` loses its pair only
//! while nothing in flight can overwrite `base.s` or drop `base`.

use super::held::run_function;
use super::modref::ModRefTable;
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::{LayoutTable, TypeLayout};
use dream_types::{DefId, TypeId, TypeInterner};

struct Types {
    i: TypeInterner,
    boxed: TypeId,
    layouts: LayoutTable,
}

fn types() -> Types {
    let mut i = TypeInterner::new();
    let boxed = i.struct_ty(DefId(7), vec![]);
    let s = i.string();
    let mut layouts = LayoutTable::default();
    layouts.insert(
        boxed,
        TypeLayout::from_fields(&i, "Box", [("s".into(), s, false, false)]),
    );
    Types { i, boxed, layouts }
}

const PEEK: DefId = DefId(40);

fn peek_call(t: &Types, x: Local) -> Statement {
    Statement::Call {
        callee: Callee {
            def: PEEK,
            args: vec![],
            ret: t.i.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(x))],
    }
}

/// `fun peek(s: borrow string)`, optionally storing into a `Box.s` it is handed.
fn peek_fn(t: &Types, stores: bool) -> MirFunction {
    let mut b = FunctionBuilder::new("peek", t.i.void());
    b.set_def(PEEK, vec![]);
    let _ = b.new_param(t.i.string(), Some("s".into()));
    if stores {
        let other = b.new_local(t.boxed, None);
        b.assign(
            Place::Field {
                base: other,
                field: 0,
            },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    }
    b.terminate(Terminator::Return(None));
    b.finish()
}

struct Snap {
    func: MirFunction,
    x: Local,
}

/// `fun f(base: borrow Box) { x = base.s; Retain(x); <mid>; peek(x); Release(x) }`.
fn snapshot(t: &Types, mid: impl FnOnce(&mut FunctionBuilder, Local)) -> Snap {
    let mut b = FunctionBuilder::new("f", t.i.void());
    let base = b.new_param(t.boxed, Some("base".into()));
    let x = b.new_local(t.i.string(), Some("x".into()));
    b.assign(
        Place::Local(x),
        Rvalue::Use(Operand::Copy(Place::Field { base, field: 0 })),
    );
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    mid(&mut b, base);
    b.push(peek_call(t, x));
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    Snap { func: b.finish(), x }
}

fn rc_ops(f: &MirFunction, x: Local) -> usize {
    f.blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter(|s| {
            matches!(s, Statement::Retain(Operand::Copy(Place::Local(l)))
                | Statement::Release(Operand::Copy(Place::Local(l))) if *l == x)
        })
        .count()
}

fn run(t: &Types, snap: &mut Snap, peek_stores: bool) {
    let mir = Mir {
        functions: vec![peek_fn(t, peek_stores)],
        ..Default::default()
    };
    let modref = ModRefTable::compute(&mir, &t.i);
    run_function(&mut snap.func, &t.i, &t.layouts, &modref);
}

#[test]
fn snapshot_read_under_a_quiet_call_drops_its_pair() {
    let t = types();
    let mut s = snapshot(&t, |_, _| {});
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, s.x), 0);
    assert!(s.func.locals[s.x.0 as usize].is_cursor);
}

#[test]
fn callee_that_may_overwrite_the_slot_keeps_the_pair() {
    let t = types();
    let mut s = snapshot(&t, |_, _| {});
    run(&t, &mut s, true);
    assert_eq!(rc_ops(&s.func, s.x), 2);
}

#[test]
fn direct_store_to_the_slot_keeps_the_pair() {
    let t = types();
    let mut s = snapshot(&t, |b, base| {
        b.assign(
            Place::Field { base, field: 0 },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    });
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, s.x), 2);
}

#[test]
fn rebinding_the_owner_keeps_the_pair() {
    let t = types();
    let boxed = t.boxed;
    let mut s = snapshot(&t, |b, base| {
        b.assign(
            Place::Local(base),
            Rvalue::New {
                def: DefId(7),
                ty: boxed,
                ctor: None,
                args: vec![],
            },
        );
    });
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, s.x), 2);
}

#[test]
fn snapshot_handed_to_a_sink_keeps_the_pair() {
    let t = types();
    let mut s = snapshot(&t, |_, _| {});
    for b in &mut s.func.blocks {
        for st in &mut b.stmts {
            if let Statement::Call { callee, .. } = st {
                callee.take_params = vec![true];
            }
        }
    }
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, s.x), 2);
}
