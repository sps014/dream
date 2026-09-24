//! Loop-carried cursor family tests: `curr = head; loop { node = curr as Some; curr = node.next }`
//! walks without RC only while nothing in flight can overwrite a traversed slot or drop the root.

use super::modref::ModRefTable;
use super::RcInsertion;
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::{LayoutTable, TypeLayout};
use dream_types::{DefId, TypeId, TypeInterner};
use std::collections::HashSet;

struct Types {
    i: TypeInterner,
    node: TypeId,
    opt: TypeId,
    layouts: LayoutTable,
}

fn types() -> Types {
    let mut i = TypeInterner::new();
    let node = i.struct_ty(DefId(7), vec![]);
    let opt = i.union_ty(DefId(8), vec![node]);
    let mut layouts = LayoutTable::default();
    let int = i.int();
    layouts.insert(
        node,
        TypeLayout::from_fields(
            &i,
            "Node",
            [
                ("value".into(), int, false, false),
                ("next".into(), opt, false, false),
            ],
        ),
    );
    Types {
        i,
        node,
        opt,
        layouts,
    }
}

struct Walk {
    func: MirFunction,
    head: Local,
    curr: Local,
    node: Local,
}

/// `fun walk(head: borrow Option<Node>)`; `extra` runs in the arm after `curr = node.next`.
fn walk(t: &Types, owned_head: bool, extra: impl FnOnce(&mut FunctionBuilder, Local, Local)) -> Walk {
    let mut b = FunctionBuilder::new("walk", t.i.void());
    let head = if owned_head {
        let h = b.new_local(t.opt, Some("head".into()));
        b.assign(
            Place::Local(h),
            Rvalue::Call {
                callee: callee(DefId(90), t.opt, 0),
                args: vec![],
            },
        );
        h
    } else {
        b.new_param(t.opt, Some("head".into()))
    };
    let curr = b.new_local(t.opt, Some("curr".into()));
    let node = b.new_local(t.node, Some("node".into()));
    let disc = b.new_temp(t.i.int());
    let v = b.new_temp(t.i.int());
    let header = b.new_block();
    let arm = b.new_block();
    let exit = b.new_block();
    b.assign(Place::Local(curr), Rvalue::Use(Operand::Copy(Place::Local(head))));
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.assign(
        Place::Local(disc),
        Rvalue::Discriminant {
            base: Operand::Copy(Place::Local(curr)),
            ty: t.opt,
        },
    );
    b.terminate(Terminator::Switch {
        value: Operand::Copy(Place::Local(disc)),
        targets: vec![(0, arm)],
        default: exit,
    });
    b.switch_to(arm);
    b.assign(
        Place::Local(node),
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(curr)),
            ty: t.opt,
            variant: 0,
            field: 0,
        },
    );
    b.assign(
        Place::Local(v),
        Rvalue::Use(Operand::Copy(Place::Field { base: node, field: 0 })),
    );
    b.assign(
        Place::Local(curr),
        Rvalue::Use(Operand::Copy(Place::Field { base: node, field: 1 })),
    );
    extra(&mut b, head, node);
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.assign(Place::Local(v), Rvalue::Use(Operand::Copy(Place::Local(head))));
    b.terminate(Terminator::Return(None));
    Walk {
        func: b.finish(),
        head,
        curr,
        node,
    }
}

fn callee(def: DefId, ret: TypeId, arity: usize) -> Callee {
    Callee {
        def,
        args: vec![],
        ret,
        take_params: vec![false; arity],
    }
}

/// `fun <def>(n: borrow Node)`, optionally storing `n.next = null`.
fn node_fn(t: &Types, def: DefId, stores_next: bool) -> MirFunction {
    let mut b = FunctionBuilder::new(format!("f{}", def.0), t.i.void());
    b.set_def(def, vec![]);
    let n = b.new_param(t.node, Some("n".into()));
    if stores_next {
        b.assign(
            Place::Field { base: n, field: 1 },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    }
    b.terminate(Terminator::Return(None));
    b.finish()
}

fn insert(t: &Types, w: &mut Walk, modref: &ModRefTable) {
    RcInsertion::run_with_layouts(&mut w.func, &t.i, &t.layouts, &HashSet::new(), modref);
}

fn rc_ops_on(func: &MirFunction, l: Local) -> usize {
    func.blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter(|s| {
            matches!(
                s,
                Statement::Retain(Operand::Copy(Place::Local(x)))
                    | Statement::Release(Operand::Copy(Place::Local(x))) if *x == l
            )
        })
        .count()
}

#[test]
fn read_only_walk_is_a_cursor_family() {
    let t = types();
    for owned in [false, true] {
        let mut w = walk(&t, owned, |_, _, _| {});
        insert(&t, &mut w, &ModRefTable::default());
        assert!(w.func.locals[w.curr.0 as usize].is_cursor, "owned={}", owned);
        assert!(w.func.locals[w.node.0 as usize].is_cursor, "owned={}", owned);
        assert_eq!(rc_ops_on(&w.func, w.curr), 0);
        assert_eq!(rc_ops_on(&w.func, w.node), 0);
    }
}

#[test]
fn store_to_traversed_slot_keeps_owners() {
    let t = types();
    let mut w = walk(&t, false, |b, _, node| {
        b.assign(
            Place::Field { base: node, field: 1 },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    });
    insert(&t, &mut w, &ModRefTable::default());
    assert!(!w.func.locals[w.curr.0 as usize].is_cursor);
}

#[test]
fn rebinding_the_root_mid_walk_keeps_owners() {
    let t = types();
    let opt = t.opt;
    let mut w = walk(&t, true, |b, head, _| {
        b.assign(
            Place::Local(head),
            Rvalue::Call {
                callee: callee(DefId(90), opt, 0),
                args: vec![],
            },
        );
    });
    insert(&t, &mut w, &ModRefTable::default());
    assert!(!w.func.locals[w.curr.0 as usize].is_cursor);
}

#[test]
fn walk_after_the_roots_last_use_keeps_owners() {
    let t = types();
    let mut w = walk(&t, true, |_, _, _| {});
    // Drop the exit-block read of `head`: `curr = head` becomes its last use (a move).
    let exit = w.func.blocks.len() - 1;
    w.func.blocks[exit].stmts.clear();
    insert(&t, &mut w, &ModRefTable::default());
    let _ = w.head;
    assert!(!w.func.locals[w.curr.0 as usize].is_cursor);
}

#[test]
fn callee_mod_ref_decides_the_family() {
    let t = types();
    for (stores, want_cursor) in [(false, true), (true, false)] {
        let def = DefId(91);
        let void = t.i.void();
        let w = walk(&t, false, |b, _, node| {
            b.push(Statement::Call {
                callee: callee(def, void, 1),
                args: vec![Operand::Copy(Place::Local(node))],
            });
        });
        let mir = Mir {
            functions: vec![w.func, node_fn(&t, def, stores)],
            ..Default::default()
        };
        let modref = ModRefTable::compute(&mir, &t.i);
        let mut w = Walk {
            func: mir.functions.into_iter().next().unwrap(),
            ..w
        };
        insert(&t, &mut w, &modref);
        assert_eq!(
            w.func.locals[w.curr.0 as usize].is_cursor,
            want_cursor,
            "callee stores next: {}",
            stores
        );
    }
}
