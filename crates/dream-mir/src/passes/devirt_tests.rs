use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Mir};
use dream_hir::{InterfaceImpl, InterfaceInfo, InterfaceTable};
use dream_types::DefId;

struct World {
    interner: TypeInterner,
    a: TypeId,
    b: TypeId,
    iface: TypeId,
}

fn world() -> World {
    let mut interner = TypeInterner::new();
    let a = interner.struct_ty(DefId(10), vec![]);
    let b = interner.struct_ty(DefId(11), vec![]);
    let iface = interner.interface_ty(DefId(12), vec![]);
    World {
        interner,
        a,
        b,
        iface,
    }
}

fn local(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn method(w: &World, name: &str, def: u32, this_ty: TypeId) -> MirFunction {
    let mut b = FunctionBuilder::new(name, w.interner.int());
    b.set_def(DefId(def), vec![]);
    b.new_param(this_ty, Some("this".into()));
    b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(
        def as i64,
    )))));
    b.finish()
}

fn module(w: &World, caller: MirFunction, a_sym: &str, b_sym: &str) -> Mir {
    let sig = w.interner.int();
    Mir {
        functions: vec![caller, method(w, "A_f", 1, w.a), method(w, "B_f", 2, w.b)],
        interfaces: InterfaceTable {
            interfaces: vec![InterfaceInfo {
                name: "I".into(),
                method_count: 1,
                sigs: vec![sig],
            }],
            impls: vec![
                InterfaceImpl {
                    class_ty: w.a,
                    entries: vec![(0, vec![a_sym.into()])],
                },
                InterfaceImpl {
                    class_ty: w.b,
                    entries: vec![(0, vec![b_sym.into()])],
                },
            ],
        },
        ..Default::default()
    }
}

fn new_of(def: u32, ty: TypeId) -> Rvalue {
    Rvalue::New {
        def: DefId(def),
        ty,
        ctor: None,
        args: vec![],
    }
}

fn iface_call(w: &World, recv: Local) -> Rvalue {
    Rvalue::InterfaceCall {
        receiver: local(recv),
        iface_id: 0,
        method_slot: 0,
        sig: w.interner.int(),
        args: vec![],
        ret: w.interner.int(),
    }
}

/// The def of the direct callee at `(block, stmt)`, or `None` if it is still an interface call.
fn direct_def(mir: &Mir, block: u32, stmt: usize) -> Option<u32> {
    match &mir.functions[0].blocks[block as usize].stmts[stmt] {
        Statement::Assign(_, Rvalue::Call { callee, .. }) => Some(callee.def.0),
        Statement::Assign(_, Rvalue::InterfaceCall { .. }) => None,
        other => panic!("unexpected {:?}", other),
    }
}

#[test]
fn new_through_cast_devirtualizes_to_that_class() {
    let w = world();
    let mut b = FunctionBuilder::new("caller", w.interner.int());
    let obj = b.new_temp(w.a);
    let recv = b.new_local(w.iface, Some("s".into()));
    let out = b.new_temp(w.interner.int());
    b.assign(Place::Local(obj), new_of(10, w.a));
    b.assign(Place::Local(recv), Rvalue::Cast(local(obj), w.a, w.iface));
    b.assign(Place::Local(out), iface_call(&w, recv));
    b.terminate(Terminator::Return(Some(local(out))));
    let mut mir = module(&w, b.finish(), "A_f", "B_f");
    assert!(Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, 0, 2), Some(1));
}

#[test]
fn unknown_receiver_param_stays_dynamic() {
    let w = world();
    let mut b = FunctionBuilder::new("caller", w.interner.int());
    let recv = b.new_param(w.iface, Some("s".into()));
    let out = b.new_temp(w.interner.int());
    b.assign(Place::Local(out), iface_call(&w, recv));
    b.terminate(Terminator::Return(Some(local(out))));
    let mut mir = module(&w, b.finish(), "A_f", "B_f");
    assert!(!Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, 0, 0), None);
}

/// `s = cond ? new A : new B; s.f()` — the join has two classes, so no exact type.
fn diamond(w: &World, then_ty: TypeId, else_ty: TypeId) -> MirFunction {
    let mut b = FunctionBuilder::new("caller", w.interner.int());
    let cond = b.new_param(w.interner.bool(), Some("c".into()));
    let recv = b.new_local(w.iface, Some("s".into()));
    let out = b.new_temp(w.interner.int());
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.terminate(Terminator::If {
        cond: local(cond),
        then_blk,
        else_blk,
    });
    for (blk, ty) in [(then_blk, then_ty), (else_blk, else_ty)] {
        b.switch_to(blk);
        let def = if ty == w.a { 10 } else { 11 };
        b.assign(Place::Local(recv), new_of(def, ty));
        b.terminate(Terminator::Goto(join));
    }
    b.switch_to(join);
    b.assign(Place::Local(out), iface_call(w, recv));
    b.terminate(Terminator::Return(Some(local(out))));
    b.finish()
}

#[test]
fn join_of_distinct_classes_stays_dynamic() {
    let w = world();
    let mut mir = module(&w, diamond(&w, w.a, w.b), "A_f", "B_f");
    assert!(!Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, 3, 0), None);
}

#[test]
fn join_of_same_class_devirtualizes() {
    let w = world();
    let mut mir = module(&w, diamond(&w, w.b, w.b), "A_f", "B_f");
    assert!(Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, 3, 0), Some(2));
}

#[test]
fn loop_reassignment_kills_the_fact() {
    let w = world();
    let mut b = FunctionBuilder::new("caller", w.interner.int());
    let cond = b.new_param(w.interner.bool(), Some("c".into()));
    let recv = b.new_local(w.iface, Some("s".into()));
    let out = b.new_temp(w.interner.int());
    let head = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(Place::Local(recv), new_of(10, w.a));
    b.terminate(Terminator::Goto(head));
    b.switch_to(head);
    b.assign(Place::Local(out), iface_call(&w, recv));
    b.terminate(Terminator::If {
        cond: local(cond),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(Place::Local(recv), new_of(11, w.b));
    b.terminate(Terminator::Goto(head));
    b.switch_to(exit);
    b.terminate(Terminator::Return(Some(local(out))));
    let mut mir = module(&w, b.finish(), "A_f", "B_f");
    assert!(!Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, head.0, 0), None);
}

#[test]
fn overwrite_after_new_uses_the_latest_def() {
    let w = world();
    let mut b = FunctionBuilder::new("caller", w.interner.int());
    let other = b.new_param(w.iface, Some("o".into()));
    let recv = b.new_local(w.iface, Some("s".into()));
    let out = b.new_temp(w.interner.int());
    b.assign(Place::Local(recv), new_of(10, w.a));
    b.assign(Place::Local(out), iface_call(&w, recv));
    b.assign(Place::Local(recv), Rvalue::Use(local(other)));
    b.assign(Place::Local(out), iface_call(&w, recv));
    b.terminate(Terminator::Return(Some(local(out))));
    let mut mir = module(&w, b.finish(), "A_f", "B_f");
    assert!(Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, 0, 1), Some(1));
    assert_eq!(direct_def(&mir, 0, 3), None);
}

#[test]
fn shared_slot_symbol_devirtualizes_without_type_facts() {
    let w = world();
    let mut b = FunctionBuilder::new("caller", w.interner.int());
    let recv = b.new_param(w.iface, Some("s".into()));
    let out = b.new_temp(w.interner.int());
    b.assign(Place::Local(out), iface_call(&w, recv));
    b.terminate(Terminator::Return(Some(local(out))));
    let mut mir = module(&w, b.finish(), "A_f", "A_f");
    assert!(Devirt.run(&mut mir, &w.interner));
    assert_eq!(direct_def(&mir, 0, 0), Some(1));
}
