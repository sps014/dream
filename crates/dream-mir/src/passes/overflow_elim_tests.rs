use super::*;
use crate::build::FunctionBuilder;
use crate::Const;
use dream_types::TypeInterner;

fn local(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn int(v: i64) -> Operand {
    Operand::Const(Const::Int(v))
}

/// `i = 0; while i < n { i = i + step }; return i` with the increment checked.
fn counted_loop(i: &TypeInterner, step: i64, cmp: BinOp) -> MirFunction {
    let mut b = FunctionBuilder::new("f", i.int());
    let n = b.new_param(i.int(), Some("n".into()));
    let ctr = b.new_local(i.int(), Some("i".into()));
    let c = b.new_temp(i.bool());
    let head = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.assign(Place::Local(ctr), Rvalue::Use(int(0)));
    b.terminate(Terminator::Goto(head));
    b.switch_to(head);
    b.assign(Place::Local(c), Rvalue::Binary(cmp, local(ctr), local(n)));
    b.terminate(Terminator::If {
        cond: local(c),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(ctr),
        Rvalue::CheckedBinary(BinOp::Add, local(ctr), int(step)),
    );
    b.terminate(Terminator::Goto(head));
    b.switch_to(exit);
    b.terminate(Terminator::Return(Some(local(ctr))));
    b.finish()
}

fn body_rvalue(func: &MirFunction) -> &Rvalue {
    match &func.blocks[2].stmts[0] {
        Statement::Assign(_, rv) => rv,
        other => panic!("unexpected {:?}", other),
    }
}

#[test]
fn counted_loop_increment_is_unchecked() {
    let i = TypeInterner::new();
    let mut func = counted_loop(&i, 1, BinOp::Lt);
    assert!(OverflowElim.run(&mut func, &i));
    assert!(matches!(body_rvalue(&func), Rvalue::Binary(BinOp::Add, ..)));
}

#[test]
fn inclusive_bound_keeps_the_check() {
    let i = TypeInterner::new();
    let mut func = counted_loop(&i, 1, BinOp::Le);
    assert!(!OverflowElim.run(&mut func, &i));
    assert!(matches!(body_rvalue(&func), Rvalue::CheckedBinary(..)));
}

#[test]
fn large_step_keeps_the_check() {
    let i = TypeInterner::new();
    let mut func = counted_loop(&i, 2, BinOp::Lt);
    assert!(!OverflowElim.run(&mut func, &i));
}

#[test]
fn masked_and_length_operands_are_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let x = b.new_param(i.int(), None);
    let arr = b.new_param(arr_ty, None);
    let lo = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let sum = b.new_temp(i.int());
    b.assign(
        Place::Local(lo),
        Rvalue::Binary(BinOp::BitAnd, local(x), int(0xff)),
    );
    b.assign(Place::Local(len), Rvalue::ArrayLen(local(arr)));
    b.assign(
        Place::Local(sum),
        Rvalue::CheckedBinary(BinOp::Sub, local(len), local(lo)),
    );
    b.terminate(Terminator::Return(Some(local(sum))));
    let mut func = b.finish();
    assert!(OverflowElim.run(&mut func, &i));
    assert!(matches!(
        &func.blocks[0].stmts[2],
        Statement::Assign(_, Rvalue::Binary(BinOp::Sub, ..))
    ));
}

#[test]
fn unbounded_parameters_keep_the_check() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let x = b.new_param(i.int(), None);
    let y = b.new_param(i.int(), None);
    let r = b.new_temp(i.int());
    let neg = b.new_temp(i.int());
    b.assign(
        Place::Local(r),
        Rvalue::CheckedBinary(BinOp::Mul, local(x), local(y)),
    );
    b.assign(Place::Local(neg), Rvalue::CheckedNeg(local(x)));
    b.terminate(Terminator::Return(Some(local(r))));
    let mut func = b.finish();
    assert!(!OverflowElim.run(&mut func, &i));
}

#[test]
fn guard_on_else_edge_bounds_decrement() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let x = b.new_param(i.int(), None);
    let c = b.new_temp(i.bool());
    let r = b.new_temp(i.int());
    let pos = b.new_block();
    let neg = b.new_block();
    b.assign(Place::Local(c), Rvalue::Binary(BinOp::Le, local(x), int(0)));
    b.terminate(Terminator::If {
        cond: local(c),
        then_blk: neg,
        else_blk: pos,
    });
    b.switch_to(pos);
    b.assign(
        Place::Local(r),
        Rvalue::CheckedBinary(BinOp::Sub, local(x), int(1)),
    );
    b.terminate(Terminator::Return(Some(local(r))));
    b.switch_to(neg);
    b.terminate(Terminator::Return(Some(int(0))));
    let mut func = b.finish();
    assert!(OverflowElim.run(&mut func, &i));
}
