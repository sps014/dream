//! Static counts of one local-only allocation: deaths are the releases that drop the last
//! count, and any path that could leak, double-free, or hand the count away is refused.

use super::object_life::{Lifetime, lifetime};
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId, TypeInterner};

const NODE: DefId = DefId::root(7);

fn copy(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn new_node(ty: TypeId) -> Rvalue {
    Rvalue::New {
        def: NODE,
        ty,
        ctor: None,
        args: vec![],
    }
}

/// `o = new Node; a = o; <body>` with members `[o, a]`.
fn run(body: impl FnOnce(&mut FunctionBuilder, &TypeInterner, Local, Local)) -> Option<Lifetime> {
    let mut i = TypeInterner::new();
    let ty = i.struct_ty(NODE, vec![]);
    let mut b = FunctionBuilder::new("f", i.void());
    let o = b.new_temp(ty);
    let a = b.new_temp(ty);
    b.assign(Place::Local(o), new_node(ty));
    b.assign(Place::Local(a), Rvalue::Use(copy(o)));
    body(&mut b, &i, o, a);
    let f = b.finish();
    lifetime(&f, &[o, a], o)
}

#[test]
fn the_last_release_is_the_death() {
    let life = run(|b, _, o, a| {
        b.push(Statement::Retain(copy(a)));
        b.push(Statement::Release(copy(o)));
        b.push(Statement::Release(copy(a)));
        b.terminate(Terminator::Return(None));
    })
    .expect("balanced");
    assert_eq!(life.deaths, vec![(0, 4, Local(1))]);
    assert_eq!(life.rc_ops, vec![(0, 2), (0, 3)]);
}

#[test]
fn releasing_a_stale_alias_is_refused() {
    assert!(
        run(|b, _, o, a| {
            b.push(Statement::Release(copy(o)));
            b.push(Statement::Release(copy(a)));
            b.terminate(Terminator::Return(None));
        })
        .is_none()
    );
}

#[test]
fn returning_with_a_live_count_is_refused() {
    assert!(run(|b, _, _, _| b.terminate(Terminator::Return(None))).is_none());
}

#[test]
fn a_take_argument_is_refused() {
    assert!(
        run(|b, i, o, _| {
            b.push(Statement::Call {
                callee: Callee {
                    borrowed_result: false,
                    def: DefId::root(40),
                    args: vec![],
                    ret: i.void(),
                    take_params: vec![true],
                },
                args: vec![copy(o)],
            });
            b.terminate(Terminator::Return(None));
        })
        .is_none()
    );
}

/// `loop { o = new; a = o; release o; o = null }`: the header sees `a` null on entry and stale
/// on the back edge, and each iteration's instance dies at its release.
#[test]
fn a_stale_alias_joins_null_at_a_loop_header() {
    let mut i = TypeInterner::new();
    let ty = i.struct_ty(NODE, vec![]);
    let mut b = FunctionBuilder::new("f", i.void());
    let c = b.new_param(i.bool(), Some("c".into()));
    let o = b.new_temp(ty);
    let a = b.new_temp(ty);
    let head = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.terminate(Terminator::Goto(head));
    b.switch_to(head);
    b.terminate(Terminator::If {
        cond: copy(c),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(Place::Local(o), new_node(ty));
    b.assign(Place::Local(a), Rvalue::Use(copy(o)));
    b.push(Statement::Release(copy(o)));
    b.assign(Place::Local(o), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::Goto(head));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let f = b.finish();
    let life = lifetime(&f, &[o, a], o).expect("one death per iteration");
    assert_eq!(life.deaths, vec![(body.0 as usize, 2, o)]);
    assert!(life.rc_ops.is_empty());
}

#[test]
fn branch_dependent_holders_preserve_one_unconditional_death() {
    let life = run(|b, i, o, a| {
        let yes = b.new_block();
        let no = b.new_block();
        let join = b.new_block();
        let c = b.new_param(i.bool(), None);
        b.terminate(Terminator::If {
            cond: copy(c),
            then_blk: yes,
            else_blk: no,
        });
        b.switch_to(yes);
        b.assign(Place::Local(a), Rvalue::Use(Operand::Const(Const::Null)));
        b.terminate(Terminator::Goto(join));
        b.switch_to(no);
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.push(Statement::Release(copy(o)));
        b.terminate(Terminator::Return(None));
    })
    .expect("the alias can be null or live while the owner has one token");
    assert_eq!(life.deaths.len(), 1);
}

#[test]
fn select_can_hold_an_alias_or_null_without_taking_a_token() {
    let life = run(|b, i, o, a| {
        let c = b.new_param(i.bool(), None);
        b.assign(
            Place::Local(a),
            Rvalue::Select {
                cond: copy(c),
                then_val: copy(o),
                else_val: Operand::Const(Const::Null),
            },
        );
        b.push(Statement::Release(copy(o)));
        b.terminate(Terminator::Return(None));
    })
    .expect("select does not change the allocation's token");
    assert_eq!(life.deaths.len(), 1);
}

#[test]
fn retaining_a_nullable_alias_preserves_one_consistent_final_cleanup() {
    let life = run(|b, i, o, a| {
        let c = b.new_param(i.bool(), None);
        b.assign(
            Place::Local(a),
            Rvalue::Select {
                cond: copy(c),
                then_val: copy(o),
                else_val: Operand::Const(Const::Null),
            },
        );
        b.push(Statement::Retain(copy(a)));
        b.push(Statement::Release(copy(a)));
        b.push(Statement::Release(copy(o)));
        b.terminate(Terminator::Return(None));
    })
    .expect("null RC is a no-op and the root always supplies the final cleanup");
    assert_eq!(life.deaths.len(), 1);
    assert_eq!(life.rc_ops.len(), 2);
}

#[test]
fn a_release_that_is_final_on_only_one_path_is_refused() {
    assert!(
        run(|b, i, o, a| {
            let yes = b.new_block();
            let no = b.new_block();
            let join = b.new_block();
            let c = b.new_param(i.bool(), None);
            b.terminate(Terminator::If {
                cond: copy(c),
                then_blk: yes,
                else_blk: no,
            });
            b.switch_to(yes);
            b.push(Statement::Retain(copy(a)));
            b.terminate(Terminator::Goto(join));
            b.switch_to(no);
            b.terminate(Terminator::Goto(join));
            b.switch_to(join);
            b.push(Statement::Release(copy(o)));
            b.terminate(Terminator::Unreachable);
        })
        .is_none()
    );
}
