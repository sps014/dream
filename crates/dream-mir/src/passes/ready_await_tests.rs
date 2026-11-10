use super::ready_await::run;
use crate::build::FunctionBuilder;
use crate::{
    BinOp, BlockId, Callee, Const, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_hir::{HFunction, InlineHint};
use dream_types::{DefId, TypeId, TypeInterner};

fn local(l: crate::Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn stub(def: u32, ret: TypeId) -> MirFunction {
    let mut b = FunctionBuilder::new(format!("f{def}"), ret);
    b.set_async(true);
    b.set_def(DefId::root(def), vec![]);
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    f.hir_fn = Some(HFunction {
        borrowed_result: false,
        def: DefId::root(def),
        name: f.name.clone(),
        symbol: f.name.clone(),
        instance: vec![],
        params: vec![],
        ret,
        locals: vec![],
        body: vec![],
        is_async: true,
        file: None,
        inline: InlineHint::Default,
    });
    f
}

/// `async fun f1(x: int): int` whose poll either completes at once or awaits first.
fn callee_poll(i: &TypeInterner, suspends: bool) -> MirFunction {
    let int = i.int();
    let mut b = FunctionBuilder::new("f1", int);
    b.set_async(true);
    b.set_def(DefId::root(1), vec![]);
    let x = b.new_take_param(int, Some("x".into()));
    let sum = b.new_temp(int);
    if suspends {
        let resume = b.new_block();
        b.terminate(Terminator::Await {
            future: Operand::Const(Const::Null),
            dest: None,
            resume,
        });
        b.switch_to(resume);
    }
    b.assign(
        Place::Local(sum),
        Rvalue::Binary(BinOp::Add, local(x), Operand::Const(Const::Int(1))),
    );
    b.terminate(Terminator::AsyncComplete(Some(local(sum))));
    b.finish()
}

/// `async fun f2(): int { return f1(41).await; }`, optionally reading the future a second time.
fn caller_poll(i: &TypeInterner, future_ty: TypeId, reuse: bool) -> MirFunction {
    let int = i.int();
    let mut b = FunctionBuilder::new("f2", int);
    b.set_async(true);
    b.set_def(DefId::root(2), vec![]);
    let fut = b.new_temp(future_ty);
    let value = b.new_temp(int);
    let resume = b.new_block();
    b.assign(
        Place::Local(fut),
        Rvalue::Call {
            callee: Callee {
                borrowed_result: false,
                def: DefId::root(1),
                args: vec![],
                ret: future_ty,
                take_params: vec![true],
            },
            args: vec![Operand::Const(Const::Int(41))],
        },
    );
    if reuse {
        let alias = b.new_temp(future_ty);
        b.assign(Place::Local(alias), Rvalue::Use(local(fut)));
        b.assign(
            Place::Local(fut),
            Rvalue::Call {
                callee: Callee {
                    borrowed_result: false,
                    def: DefId::root(1),
                    args: vec![],
                    ret: future_ty,
                    take_params: vec![true],
                },
                args: vec![Operand::Const(Const::Int(41))],
            },
        );
    }
    b.terminate(Terminator::Await {
        future: local(fut),
        dest: Some(value),
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::AsyncComplete(Some(local(value))));
    b.finish()
}

fn module(suspends: bool, reuse: bool) -> (Mir, TypeInterner) {
    let i = TypeInterner::new();
    let int = i.int();
    let future_ty = i.object();
    let mir = Mir {
        functions: vec![stub(1, int), stub(2, int)],
        polls: vec![callee_poll(&i, suspends), caller_poll(&i, future_ty, reuse)],
        ..Default::default()
    };
    (mir, i)
}

#[test]
fn immediate_await_of_a_ready_body_calls_a_sync_copy() {
    let (mut mir, i) = module(false, false);
    assert!(run(&mut mir, &i));
    assert_eq!(mir.functions.len(), 3);
    let copy = &mir.functions[2];
    assert!(!copy.is_async);
    assert_eq!(copy.ret, i.int());
    assert!(matches!(
        copy.blocks[0].terminator,
        Terminator::Return(Some(_))
    ));
    assert!(copy.locals[copy.params[0].0 as usize].is_take);

    let caller = &mir.polls[1];
    let Statement::Assign(Place::Local(dest), Rvalue::Call { callee, args }) =
        &caller.blocks[0].stmts[0]
    else {
        panic!("not a direct call: {:?}", caller.blocks[0].stmts)
    };
    assert_eq!(dest.0, 1);
    assert_eq!(callee.def, copy.def);
    assert_eq!(callee.ret, i.int());
    assert_eq!(callee.take_params, vec![true]);
    assert_eq!(args.len(), 1);
    assert!(matches!(
        caller.blocks[0].terminator,
        Terminator::Goto(BlockId(1))
    ));
}

#[test]
fn bodies_that_can_suspend_keep_their_future() {
    let (mut mir, i) = module(true, false);
    assert!(!run(&mut mir, &i));
    assert_eq!(mir.functions.len(), 2);
    assert!(matches!(
        mir.polls[1].blocks[0].terminator,
        Terminator::Await { .. }
    ));
}

#[test]
fn futures_read_elsewhere_keep_their_future() {
    let (mut mir, i) = module(false, true);
    assert!(!run(&mut mir, &i));
    assert!(matches!(
        mir.polls[1].blocks[0].terminator,
        Terminator::Await { .. }
    ));
}

#[test]
fn bodies_reaching_the_task_runtime_keep_their_future() {
    let (mut mir, i) = module(false, false);
    let int = i.int();
    mir.intrinsics = vec![(DefId::root(9), "dream_task_protect".into())];
    mir.polls[0].blocks[0].stmts.insert(
        0,
        Statement::Call {
            callee: Callee {
                borrowed_result: false,
                def: DefId::root(9),
                args: vec![],
                ret: int,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    assert!(!run(&mut mir, &i));
}

fn call(def: u32, ret: TypeId, args: Vec<Operand>, take: Vec<bool>) -> Rvalue {
    Rvalue::Call {
        callee: Callee {
            borrowed_result: false,
            def: DefId::root(def),
            args: vec![],
            ret,
            take_params: take,
        },
        args,
    }
}

/// `async fun f3(promises, token)`: `check(token); return promise_all(promises).await;`.
fn join_poll(i: &TypeInterner, future_ty: TypeId, ints: TypeId) -> MirFunction {
    let mut b = FunctionBuilder::new("f3", ints);
    b.set_async(true);
    b.set_def(DefId::root(3), vec![]);
    let promises = b.new_param(future_ty, Some("promises".into()));
    let token = b.new_take_param(i.object(), Some("token".into()));
    let all = b.new_temp(future_ty);
    let settled = b.new_temp(ints);
    let resume = b.new_block();
    b.push(Statement::Call {
        callee: Callee {
            borrowed_result: false,
            def: DefId::root(8),
            args: vec![],
            ret: i.void(),
            take_params: vec![true],
        },
        args: vec![local(token)],
    });
    b.assign(
        Place::Local(all),
        call(7, future_ty, vec![local(promises)], vec![false]),
    );
    b.terminate(Terminator::Await {
        future: local(all),
        dest: Some(settled),
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::AsyncComplete(Some(local(settled))));
    b.finish()
}

/// `async fun f2(): int[] { return f3([f1(n), f1(n + 1)], null).await; }`, optionally rebinding
/// `n` between building the first child and awaiting the join.
fn join_caller(i: &TypeInterner, future_ty: TypeId, ints: TypeId, rebind: bool) -> MirFunction {
    let int = i.int();
    let mut b = FunctionBuilder::new("f2", ints);
    b.set_async(true);
    b.set_def(DefId::root(2), vec![]);
    let n = b.new_temp(int);
    let next = b.new_temp(int);
    let first = b.new_temp(future_ty);
    let second = b.new_temp(future_ty);
    let array = b.new_temp(future_ty);
    let token = b.new_temp(i.object());
    let join = b.new_temp(future_ty);
    let values = b.new_temp(ints);
    let resume = b.new_block();
    b.assign(Place::Local(n), Rvalue::Use(Operand::Const(Const::Int(4))));
    b.assign(
        Place::Local(first),
        call(1, future_ty, vec![local(n)], vec![true]),
    );
    b.assign(
        Place::Local(next),
        Rvalue::Binary(BinOp::Add, local(n), Operand::Const(Const::Int(1))),
    );
    if rebind {
        b.assign(Place::Local(n), Rvalue::Use(Operand::Const(Const::Int(9))));
    }
    b.assign(
        Place::Local(second),
        call(1, future_ty, vec![local(next)], vec![true]),
    );
    b.assign(
        Place::Local(array),
        Rvalue::ArrayLit {
            elem_ty: future_ty,
            elems: vec![local(first), local(second)],
        },
    );
    b.assign(
        Place::Local(token),
        Rvalue::Use(Operand::Const(Const::Null)),
    );
    b.assign(
        Place::Local(join),
        call(
            3,
            future_ty,
            vec![local(array), local(token)],
            vec![false, true],
        ),
    );
    b.terminate(Terminator::Await {
        future: local(join),
        dest: Some(values),
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::AsyncComplete(Some(local(values))));
    b.finish()
}

fn join_module(rebind: bool) -> (Mir, TypeInterner) {
    let mut i = TypeInterner::new();
    let int = i.int();
    let ints = i.array(int);
    let future_ty = i.object();
    let mir = Mir {
        functions: vec![stub(1, int), stub(2, ints), stub(3, ints)],
        polls: vec![
            callee_poll(&i, false),
            join_caller(&i, future_ty, ints, rebind),
            join_poll(&i, future_ty, ints),
        ],
        intrinsics: vec![(DefId::root(7), "promise_all".into())],
        ..Default::default()
    };
    (mir, i)
}

#[test]
fn joins_of_ready_children_run_checks_then_children_in_order() {
    let (mut mir, i) = join_module(false);
    assert!(run(&mut mir, &i));
    let copy = mir.functions[3].def;
    let caller = &mir.polls[1];
    assert!(matches!(
        caller.blocks[0].terminator,
        Terminator::Goto(BlockId(1))
    ));
    let tail: Vec<&Statement> = caller.blocks[0].stmts.iter().rev().take(4).rev().collect();
    let [check, a, b, array] = tail.as_slice() else {
        panic!("{:?}", caller.blocks[0].stmts)
    };
    assert!(matches!(check, Statement::Call { callee, args }
        if callee.def == DefId::root(8)
            && matches!(args.as_slice(), [Operand::Copy(Place::Local(crate::Local(5)))])));
    for (stmt, arg) in [(a, 0), (b, 1)] {
        assert!(
            matches!(stmt, Statement::Assign(_, Rvalue::Call { callee, args })
            if callee.def == copy
                && matches!(args.as_slice(), [Operand::Copy(Place::Local(l))] if l.0 == arg))
        );
    }
    assert!(
        matches!(array, Statement::Assign(Place::Local(crate::Local(7)), Rvalue::ArrayLit { elem_ty, elems })
        if *elem_ty == i.int() && elems.len() == 2)
    );
    assert!(!caller.blocks[0].stmts.iter().any(|s| matches!(
        s,
        Statement::Assign(_, Rvalue::Call { callee, .. }) if callee.def == DefId::root(3) || callee.def == DefId::root(1)
    )));
}

#[test]
fn joins_whose_child_arguments_change_before_the_await_keep_their_futures() {
    let (mut mir, i) = join_module(true);
    run(&mut mir, &i);
    assert!(matches!(
        mir.polls[1].blocks[0].terminator,
        Terminator::Await { .. }
    ));
}
