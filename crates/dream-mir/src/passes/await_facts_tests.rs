use super::{GlobalProp, Gvn, MirPass};
use crate::build::FunctionBuilder;
use crate::{BinOp, Const, Local, Operand, Place, Rvalue, Terminator};
use dream_types::TypeInterner;

fn copy(local: Local) -> Operand {
    Operand::Copy(Place::Local(local))
}

#[test]
fn await_invalidates_copy_facts_naming_the_destination() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("resume_copy", i.int());
    let future = b.new_param(i.int(), None);
    let old = b.new_param(i.int(), None);
    let result = b.new_temp(i.int());
    let alias = b.new_temp(i.int());
    let resume = b.new_block();
    b.assign(Place::Local(result), Rvalue::Use(copy(old)));
    b.assign(Place::Local(alias), Rvalue::Use(copy(result)));
    b.terminate(Terminator::Await {
        future: copy(future),
        dest: Some(result),
        resume,
    });
    b.switch_to(resume);
    b.terminate(Terminator::Return(Some(copy(result))));
    let mut f = b.finish();
    GlobalProp.run(&mut f, &i);
    assert!(
        matches!(f.blocks[resume.0 as usize].terminator, Terminator::Return(Some(Operand::Copy(Place::Local(l)))) if l == result)
    );
}

#[test]
fn await_invalidates_expressions_reading_or_defined_into_the_destination() {
    for destination_is_result in [false, true] {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("resume_cse", i.int());
        let future = b.new_param(i.int(), None);
        let input = b.new_param(i.int(), None);
        let before = b.new_temp(i.int());
        let after = b.new_temp(i.int());
        let resume = b.new_block();
        let expression = || Rvalue::Binary(BinOp::Add, copy(input), Operand::Const(Const::Int(1)));
        b.assign(Place::Local(before), expression());
        b.terminate(Terminator::Await {
            future: copy(future),
            dest: Some(if destination_is_result { before } else { input }),
            resume,
        });
        b.switch_to(resume);
        b.assign(Place::Local(after), expression());
        b.terminate(Terminator::Return(Some(copy(after))));
        let mut f = b.finish();
        Gvn.run(&mut f, &i);
        assert!(matches!(
            f.blocks[resume.0 as usize].stmts[0],
            crate::Statement::Assign(_, Rvalue::Binary(..))
        ));
    }
}
