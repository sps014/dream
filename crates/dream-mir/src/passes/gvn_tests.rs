use super::*;
use crate::build::FunctionBuilder;
use crate::{Operand, Place, Rvalue, Terminator};

#[test]
fn dedups_repeated_binary() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let x = b.new_param(i.int(), Some("x".into()));
    let a = b.new_temp(i.int());
    let c = b.new_temp(i.int());
    let mul = || {
        Rvalue::Binary(
            BinOp::Mul,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Int(4)),
        )
    };
    b.assign(Place::Local(a), mul());
    b.assign(Place::Local(c), mul());
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
    let mut func = b.finish();
    assert!(Gvn.run(&mut func, &i));
    // Second `x*4` becomes `Use(a)`.
    match &func.blocks[0].stmts[1] {
        Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Local(l)))) => assert_eq!(*l, a),
        other => panic!("expected CSE copy, got {:?}", other),
    }
}

#[test]
fn invalidates_on_operand_redef() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let x = b.new_local(i.int(), Some("x".into()));
    let a = b.new_temp(i.int());
    let c = b.new_temp(i.int());
    b.assign(
        Place::Local(a),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Int(1)),
        ),
    );
    // Redefine x between the two adds: the second must NOT be CSE'd.
    b.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Int(9))));
    b.assign(
        Place::Local(c),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
    let mut func = b.finish();
    Gvn.run(&mut func, &i);
    assert!(matches!(
        &func.blocks[0].stmts[2],
        Statement::Assign(_, Rvalue::Binary(..))
    ));
}

/// Two loops in one function (inlined `foreach` after IV): the first loop's bump pointer is
/// initialized to `x+4` then mutated on the latch. A later `x+4` must not CSE to that pointer.
#[test]
fn does_not_cse_add_across_loop_back_edge() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let x = b.new_param(i.int(), Some("x".into()));
    let c = b.new_param(i.bool(), Some("c".into()));
    let p = b.new_temp(i.int());
    let q = b.new_temp(i.int());
    let header = b.new_block();
    let latch = b.new_block();
    let after = b.new_block();
    let add4 = || {
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Int(4)),
        )
    };
    b.assign(Place::Local(p), add4());
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: latch,
        else_blk: after,
    });
    b.switch_to(latch);
    b.assign(
        Place::Local(p),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(p)),
            Operand::Const(Const::Int(4)),
        ),
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(after);
    b.assign(Place::Local(q), add4());
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(q)))));
    let mut func = b.finish();
    Gvn.run(&mut func, &i);
    match &func.blocks[after.0 as usize].stmts[0] {
        Statement::Assign(_, Rvalue::Binary(..)) => {}
        other => panic!(
            "post-loop x+4 must not CSE to the loop bump pointer, got {:?}",
            other
        ),
    }
}

#[test]
fn does_not_cse_string_eq_with_different_literals() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.bool());
    let s = b.new_param(i.string(), Some("s".into()));
    let a = b.new_temp(i.bool());
    let c = b.new_temp(i.bool());
    b.assign(
        Place::Local(a),
        Rvalue::Binary(
            BinOp::Eq,
            Operand::Copy(Place::Local(s)),
            Operand::Const(Const::Str("int".into())),
        ),
    );
    b.assign(
        Place::Local(c),
        Rvalue::Binary(
            BinOp::Eq,
            Operand::Copy(Place::Local(s)),
            Operand::Const(Const::Str("string".into())),
        ),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
    let mut func = b.finish();
    Gvn.run(&mut func, &i);
    match &func.blocks[0].stmts[1] {
        Statement::Assign(_, Rvalue::Binary(BinOp::Eq, _, _)) => {}
        other => panic!(
            "s == \"string\" must not CSE to s == \"int\", got {:?}",
            other
        ),
    }
}

#[test]
fn dedups_repeated_float_binary() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.double());
    let x = b.new_param(i.double(), Some("x".into()));
    let a = b.new_temp(i.double());
    let c = b.new_temp(i.double());
    let scale = || {
        Rvalue::Binary(
            BinOp::Mul,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Float(2.5)),
        )
    };
    b.assign(Place::Local(a), scale());
    b.assign(Place::Local(c), scale());
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
    let mut func = b.finish();
    assert!(Gvn.run(&mut func, &i));
    match &func.blocks[0].stmts[1] {
        Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Local(l)))) => assert_eq!(*l, a),
        other => panic!("expected float CSE copy, got {:?}", other),
    }
}

/// `x / 0.0` (+inf) and `x / -0.0` (-inf) differ; the bit-keyed float constants must not
/// share a GVN entry.
#[test]
fn distinguishes_signed_zero_divisors() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.double());
    let x = b.new_param(i.double(), Some("x".into()));
    let a = b.new_temp(i.double());
    let c = b.new_temp(i.double());
    b.assign(
        Place::Local(a),
        Rvalue::Binary(
            BinOp::Div,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Float(0.0)),
        ),
    );
    b.assign(
        Place::Local(c),
        Rvalue::Binary(
            BinOp::Div,
            Operand::Copy(Place::Local(x)),
            Operand::Const(Const::Float(-0.0)),
        ),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
    let mut func = b.finish();
    Gvn.run(&mut func, &i);
    match &func.blocks[0].stmts[1] {
        Statement::Assign(_, Rvalue::Binary(..)) => {}
        other => panic!("1/0 must not CSE to 1/-0, got {:?}", other),
    }
}

#[test]
fn does_not_number_string_reads_or_allocating_concatenation_across_mutation() {
    for op in [BinOp::Eq, BinOp::Ne, BinOp::Add] {
        let i = TypeInterner::new();
        let result = if op == BinOp::Add {
            i.string()
        } else {
            i.bool()
        };
        let mut b = FunctionBuilder::new("strings", result);
        let left = b.new_param(i.string(), None);
        let right = b.new_param(i.string(), None);
        let first = b.new_temp(result);
        let second = b.new_temp(result);
        let expression = || {
            Rvalue::Binary(
                op,
                Operand::Copy(Place::Local(left)),
                Operand::Copy(Place::Local(right)),
            )
        };
        b.assign(Place::Local(first), expression());
        b.push(Statement::Call {
            callee: crate::Callee {
                borrowed_result: false,
                def: dream_types::DefId::root(99),
                args: vec![],
                ret: i.void(),
                take_params: vec![],
            },
            args: vec![Operand::Copy(Place::Local(right))],
        });
        b.assign(Place::Local(second), expression());
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
            second,
        )))));
        let mut func = b.finish();
        assert!(!Gvn.run(&mut func, &i));
        assert!(matches!(
            func.blocks[0].stmts[2],
            Statement::Assign(_, Rvalue::Binary(..))
        ));
    }
}
