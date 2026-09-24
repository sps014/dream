//! Borrow inference: read-only sink parameters flip to borrowed together with every call site,
//! unless a callee could free a +0 argument, a `del` could observe the change, or the parameter
//! escapes.

use super::{ModulePass, ParamModes};
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::{LayoutTable, TypeLayout};
use dream_types::{DefId, TypeId, TypeInterner};

struct Types {
    i: TypeInterner,
    node: TypeId,
    holder: TypeId,
    layouts: LayoutTable,
}

fn types() -> Types {
    let mut i = TypeInterner::new();
    let node = i.struct_ty(DefId(7), vec![]);
    let holder = i.struct_ty(DefId(8), vec![]);
    let int = i.int();
    let mut layouts = LayoutTable::default();
    layouts.insert(
        node,
        TypeLayout::from_fields(&i, "Node", [("value".into(), int, false, false)]),
    );
    layouts.insert(
        holder,
        TypeLayout::from_fields(&i, "Holder", [("node".into(), node, false, false)]),
    );
    Types {
        i,
        node,
        holder,
        layouts,
    }
}

const CALLEE: DefId = DefId(20);
const CALLER: DefId = DefId(21);

fn call(ret: TypeId, takes: Vec<bool>) -> Callee {
    Callee {
        def: CALLEE,
        args: vec![],
        ret,
        take_params: takes,
    }
}

/// `fun callee(h: Holder, n: Node): int`, reading `n.value`; `body` runs first.
fn callee_fn(t: &Types, body: impl FnOnce(&mut FunctionBuilder, Local, Local)) -> MirFunction {
    let mut b = FunctionBuilder::new("callee", t.i.int());
    b.set_def(CALLEE, vec![]);
    let h = b.new_take_param(t.holder, Some("h".into()));
    let n = b.new_take_param(t.node, Some("n".into()));
    body(&mut b, h, n);
    let v = b.new_temp(t.i.int());
    b.assign(
        Place::Local(v),
        Rvalue::Use(Operand::Copy(Place::Field { base: n, field: 0 })),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(v)))));
    b.finish()
}

/// `fun caller(h: Holder) { callee(h, <arg>) }` with `arg` a fresh `Node` or `h.node`.
fn caller_fn(t: &Types, slot_arg: bool) -> MirFunction {
    let mut b = FunctionBuilder::new("caller", t.i.void());
    b.set_def(CALLER, vec![]);
    let h = b.new_param(t.holder, Some("h".into()));
    let n = b.new_local(t.node, Some("n".into()));
    let rv = if slot_arg {
        Rvalue::Use(Operand::Copy(Place::Field { base: h, field: 0 }))
    } else {
        Rvalue::New {
            def: DefId(7),
            ty: t.node,
            ctor: None,
            args: vec![],
        }
    };
    b.assign(Place::Local(n), rv);
    let r = b.new_temp(t.i.int());
    b.assign(
        Place::Local(r),
        Rvalue::Call {
            callee: call(t.i.int(), vec![true, true]),
            args: vec![
                Operand::Copy(Place::Local(h)),
                Operand::Copy(Place::Local(n)),
            ],
        },
    );
    b.terminate(Terminator::Return(None));
    b.finish()
}

fn run(t: &Types, callee: MirFunction, caller: MirFunction, extra: Vec<MirFunction>) -> Mir {
    let mut functions = vec![callee, caller];
    functions.extend(extra);
    let mut mir = Mir {
        functions,
        layouts: t.layouts.clone(),
        ..Default::default()
    };
    let _ = ParamModes.run(&mut mir, &t.i);
    mir
}

fn param_take(mir: &Mir, pos: usize) -> bool {
    let f = &mir.functions[0];
    f.locals[f.params[pos].0 as usize].is_take
}

fn site_takes(mir: &Mir) -> Vec<bool> {
    mir.functions[1]
        .blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .find_map(|s| match s {
            Statement::Assign(_, Rvalue::Call { callee, .. }) => Some(callee.take_params.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn read_only_param_and_its_call_sites_flip_to_borrow() {
    let t = types();
    let mir = run(&t, callee_fn(&t, |_, _, _| {}), caller_fn(&t, false), vec![]);
    assert!(!param_take(&mir, 0));
    assert!(!param_take(&mir, 1));
    assert_eq!(site_takes(&mir), vec![false, false]);
}

#[test]
fn stored_param_stays_a_sink() {
    let t = types();
    let callee = callee_fn(&t, |b, h, n| {
        b.assign(
            Place::Field { base: h, field: 0 },
            Rvalue::Use(Operand::Copy(Place::Local(n))),
        );
    });
    let mir = run(&t, callee, caller_fn(&t, false), vec![]);
    assert!(param_take(&mir, 1));
    assert_eq!(site_takes(&mir), vec![false, true]);
}

#[test]
fn slot_argument_blocks_a_callee_that_overwrites_its_owner() {
    let t = types();
    let overwrite = |b: &mut FunctionBuilder, h: Local, _: Local| {
        b.assign(
            Place::Field { base: h, field: 0 },
            Rvalue::Use(Operand::Const(Const::Null)),
        );
    };
    // A fresh argument is owned by the caller across the call, so the overwrite is harmless.
    let mir = run(&t, callee_fn(&t, overwrite), caller_fn(&t, false), vec![]);
    assert!(!param_take(&mir, 1));
    // `h.node` at +0 would be freed by `h.node = null` before `n.value`.
    let mir = run(&t, callee_fn(&t, overwrite), caller_fn(&t, true), vec![]);
    assert!(param_take(&mir, 1));
    assert_eq!(site_takes(&mir), vec![false, true]);
}

#[test]
fn del_on_the_param_graph_keeps_the_sink() {
    let t = types();
    let mut b = FunctionBuilder::new("Node_del", t.i.void());
    b.set_def(DefId(30), vec![]);
    b.new_param(t.node, Some("this".into()));
    b.terminate(Terminator::Return(None));
    let mir = run(&t, callee_fn(&t, |_, _, _| {}), caller_fn(&t, false), vec![b.finish()]);
    assert!(param_take(&mir, 1));
}

#[test]
fn address_taken_or_uncalled_functions_keep_their_modes() {
    let t = types();
    let mut caller = caller_fn(&t, false);
    let r = Local(caller.locals.len() as u32);
    caller.locals.push(caller.locals[0].clone());
    caller.blocks[0].stmts.push(Statement::Assign(
        Place::Local(r),
        Rvalue::FuncRef(call(t.i.int(), vec![true, true])),
    ));
    let mir = run(&t, callee_fn(&t, |_, _, _| {}), caller, vec![]);
    assert!(param_take(&mir, 1));

    let mut mir = Mir {
        functions: vec![callee_fn(&t, |_, _, _| {})],
        layouts: t.layouts.clone(),
        ..Default::default()
    };
    let _ = ParamModes.run(&mut mir, &t.i);
    assert!(param_take(&mir, 1));
}
