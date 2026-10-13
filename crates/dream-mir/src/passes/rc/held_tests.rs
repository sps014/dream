//! Post-inline held-by-owner tests: `x = base.s; Retain(x); …; Release(x)` loses its pair only
//! while nothing in flight can overwrite `base.s` or drop `base`.

use super::held::run_function;
use crate::analysis::effects::ModRefTable;
use crate::build::FunctionBuilder;
use crate::{
    Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_hir::{LayoutTable, TypeLayout};
use dream_types::{DefId, TypeId, TypeInterner};

struct Types {
    i: TypeInterner,
    boxed: TypeId,
    layouts: LayoutTable,
}

fn types() -> Types {
    let mut i = TypeInterner::new();
    let boxed = i.struct_ty(DefId::root(7), vec![]);
    let s = i.string();
    let mut layouts = LayoutTable::default();
    layouts.insert(
        boxed,
        TypeLayout::from_fields(&i, "Box", [("s".into(), s, false, false)]),
    );
    Types { i, boxed, layouts }
}

const PEEK: DefId = DefId::root(40);

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
    Snap {
        func: b.finish(),
        x,
    }
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
                def: DefId::root(7),
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

#[test]
fn snapshot_moved_into_the_loop_binding_drops_every_pair() {
    let t = types();
    let mut b = FunctionBuilder::new("f", t.i.void());
    let base = b.new_param(t.boxed, Some("base".into()));
    let x = b.new_local(t.i.string(), None);
    let n = b.new_local(t.i.string(), Some("n".into()));
    b.assign(
        Place::Local(x),
        Rvalue::Use(Operand::Copy(Place::Field { base, field: 0 })),
    );
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.assign(Place::Local(n), Rvalue::Move { src: x, cast: None });
    b.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Null)));
    b.push(peek_call(&t, n));
    b.push(Statement::Release(Operand::Copy(Place::Local(n))));
    b.terminate(Terminator::Return(None));
    let mut s = Snap {
        func: b.finish(),
        x,
    };
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, x) + rc_ops(&s.func, n), 0);
}

/// `fun g(p: borrow string) { c = p; Retain(c); peek(c); <tail> }`.
fn param_copy(t: &Types, return_param: bool) -> Snap {
    let ret = if return_param {
        t.i.string()
    } else {
        t.i.void()
    };
    let mut b = FunctionBuilder::new("g", ret);
    let p = b.new_param(t.i.string(), Some("p".into()));
    let c = b.new_local(t.i.string(), None);
    b.assign(Place::Local(c), Rvalue::Use(Operand::Copy(Place::Local(p))));
    b.push(Statement::Retain(Operand::Copy(Place::Local(c))));
    b.push(peek_call(t, c));
    if return_param {
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(p)))));
    } else {
        b.push(Statement::Release(Operand::Copy(Place::Local(c))));
        b.terminate(Terminator::Return(None));
    }
    Snap {
        func: b.finish(),
        x: c,
    }
}

#[test]
fn copy_of_a_borrowed_param_drops_its_pair() {
    let t = types();
    let mut s = param_copy(&t, false);
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, s.x), 0);
}

#[test]
fn copy_retain_standing_in_for_a_returned_param_stays() {
    let t = types();
    let mut s = param_copy(&t, true);
    run(&t, &mut s, false);
    assert_eq!(rc_ops(&s.func, s.x), 1);
}

#[test]
fn array_record_field_borrows_only_while_the_array_slot_stays_intact() {
    for mutation in 0..4 {
        let mut t = types();
        let array_ty = t.i.array(t.boxed);
        let mut b = FunctionBuilder::new("probe", t.i.void());
        let array = b.new_param(array_ty, None);
        let slot = b.new_local(t.boxed, None);
        let x = b.new_local(t.i.string(), None);
        b.assign(
            Place::Local(slot),
            Rvalue::Use(Operand::Copy(Place::index(
                array,
                Operand::Const(Const::Int(0)),
            ))),
        );
        b.assign(
            Place::Local(x),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: slot,
                field: 0,
            })),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        match mutation {
            1 => b.assign(
                Place::Field {
                    base: slot,
                    field: 0,
                },
                Rvalue::Use(Operand::Const(Const::Null)),
            ),
            2 => b.assign(
                Place::index(array, Operand::Const(Const::Int(0))),
                Rvalue::Use(Operand::Const(Const::Null)),
            ),
            3 => b.push(Statement::Call {
                callee: Callee {
                    def: DefId::root(999),
                    args: vec![],
                    ret: t.i.void(),
                    take_params: vec![],
                },
                args: vec![],
            }),
            _ => {}
        }
        b.push(peek_call(&t, x));
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut s = Snap {
            func: b.finish(),
            x,
        };
        s.func.locals[slot.0 as usize].is_ref = true;
        run(&t, &mut s, false);
        assert_eq!(rc_ops(&s.func, x), if mutation == 0 { 0 } else { 2 });
    }
}

#[test]
fn call_replacing_the_owner_array_keeps_the_record_field_snapshot() {
    let mut t = types();
    let array_ty = t.i.array(t.boxed);
    let owner_ty = t.i.struct_ty(DefId::root(9), vec![]);
    t.layouts.insert(
        owner_ty,
        TypeLayout::from_fields(&t.i, "Owner", [("items".into(), array_ty, false, false)]),
    );
    let mut replace = FunctionBuilder::new("replace", t.i.void());
    replace.set_def(DefId::root(41), vec![]);
    let owner = replace.new_param(owner_ty, None);
    replace.assign(
        Place::Field {
            base: owner,
            field: 0,
        },
        Rvalue::Use(Operand::Const(Const::Null)),
    );
    replace.terminate(Terminator::Return(None));
    let mir = Mir {
        functions: vec![replace.finish(), peek_fn(&t, false)],
        ..Mir::default()
    };
    let effects = ModRefTable::compute(&mir, &t.i);
    let mut b = FunctionBuilder::new("read", t.i.void());
    let owner = b.new_param(owner_ty, None);
    let array = b.new_local(array_ty, None);
    let slot = b.new_local(t.boxed, None);
    let x = b.new_local(t.i.string(), None);
    b.assign(
        Place::Local(array),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: owner,
            field: 0,
        })),
    );
    b.assign(
        Place::Local(slot),
        Rvalue::Use(Operand::Copy(Place::index(
            array,
            Operand::Const(Const::Int(0)),
        ))),
    );
    b.assign(
        Place::Local(x),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: slot,
            field: 0,
        })),
    );
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.push(Statement::Call {
        callee: Callee {
            def: DefId::root(41),
            args: vec![],
            ret: t.i.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(owner))],
    });
    b.push(peek_call(&t, x));
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    f.locals[slot.0 as usize].is_ref = true;
    f.locals[array.0 as usize].is_cursor = true;
    run_function(&mut f, &t.i, &t.layouts, &effects);
    assert_eq!(rc_ops(&f, x), 2);
}
