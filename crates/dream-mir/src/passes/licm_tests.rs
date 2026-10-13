use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Rvalue};

/// entry -> cond -> (body: t = 2 + 3; goto cond | after: return 0). `t` is loop-invariant, used
/// nowhere, so LICM must hoist `t = 2 + 3` out of `body` into a preheader.
#[test]
fn hoists_invariant_out_of_loop() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let t = b.new_temp(i.int());
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.terminate(Terminator::If {
        cond: Operand::Const(Const::Bool(true)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(t),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Const(Const::Int(2)),
            Operand::Const(Const::Int(3)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
    let mut func = b.finish();

    assert!(Licm.run(&mut func, &i), "should hoist");
    // The body block (index 2) must no longer contain the assignment.
    let body_has_assign = func.blocks[2]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Assign(Place::Local(l), _) if *l == t));
    assert!(!body_has_assign, "assignment left in loop body");
    // A new preheader block holds it.
    let hoisted_somewhere = func.blocks.iter().any(|bb| {
        bb.stmts
            .iter()
            .any(|s| matches!(s, Statement::Assign(Place::Local(l), _) if *l == t))
    });
    assert!(hoisted_somewhere, "assignment vanished entirely");
}

/// A loop-carried accumulator (`s = s + 1`) reads its own destination, so it is *not* invariant
/// and must stay in the loop.
#[test]
fn does_not_hoist_loop_carried() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let s = b.new_temp(i.int());
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.assign(Place::Local(s), Rvalue::Use(Operand::Const(Const::Int(0))));
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.terminate(Terminator::If {
        cond: Operand::Const(Const::Bool(true)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(s),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(s)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
    let mut func = b.finish();

    assert!(
        !Licm.run(&mut func, &i),
        "must not hoist a loop-carried value"
    );
}

#[test]
fn does_not_hoist_string_content_reads_or_allocating_binary_results() {
    for op in [BinOp::Eq, BinOp::Ne, BinOp::Add] {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("strings", i.void());
        let left = b.new_param(i.string(), None);
        let right = b.new_param(i.string(), None);
        let condition = b.new_param(i.bool(), None);
        let result = b.new_temp(if op == BinOp::Add {
            i.string()
        } else {
            i.bool()
        });
        let header = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(condition)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(result),
            Rvalue::Binary(
                op,
                Operand::Copy(Place::Local(left)),
                Operand::Copy(Place::Local(right)),
            ),
        );
        b.push(Statement::Print {
            arg: Operand::Copy(Place::Local(result)),
            ty: if op == BinOp::Add {
                i.string()
            } else {
                i.bool()
            },
            newline: true,
        });
        b.push(Statement::Call {
            callee: crate::Callee {
                def: dream_types::DefId::root(99),
                args: vec![],
                ret: i.void(),
                take_params: vec![],
            },
            args: vec![Operand::Copy(Place::Local(right))],
        });
        b.terminate(Terminator::Goto(header));
        b.switch_to(after);
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(!Licm.run(&mut func, &i));
        assert!(matches!(
            func.blocks[body.0 as usize].stmts[0],
            Statement::Assign(_, Rvalue::Binary(..))
        ));
    }
}
