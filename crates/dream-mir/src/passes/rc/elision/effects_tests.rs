use super::*;
use crate::build::FunctionBuilder;
use crate::{Callee, Const, Mir, Operand, Rvalue, Terminator};
use dream_types::DefId;

#[test]
fn borrowed_parameter_pairs_cross_calls_but_never_transfers_or_observers() {
    for case in 0..7 {
        let i = TypeInterner::new();
        let mut helper = FunctionBuilder::new("helper", i.void());
        helper.set_def(DefId::root(820), vec![]);
        let value = helper.new_param(i.string(), None);
        if case == 1 {
            let count = helper.new_local(i.int(), None);
            helper.assign(
                Place::Local(count),
                Rvalue::ObservedLoad(Operand::Copy(Place::Local(value))),
            );
        }
        helper.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![helper.finish()],
            profile: dream_abi::profile::CompileProfile::Release,
            ..Mir::default()
        };
        let effects = ModRefTable::after_rc(&mir, &i);
        let mut b = FunctionBuilder::new("reader", i.void());
        let p = if case == 6 {
            b.new_take_param(i.string(), None)
        } else {
            b.new_param(i.string(), None)
        };
        let alias = b.new_local(i.string(), None);
        let op = |l| Operand::Copy(Place::Local(l));
        b.assign(Place::Local(alias), Rvalue::Use(op(p)));
        if case == 5 {
            b.assign(Place::Local(p), Rvalue::Use(Operand::Const(Const::Null)));
        }
        b.push(Statement::Retain(op(p)));
        let middle = b.new_block();
        let end = b.new_block();
        b.terminate(Terminator::Goto(middle));
        b.switch_to(middle);
        b.push(Statement::Call {
            callee: Callee {
                def: DefId::root(if case == 4 { 999 } else { 820 }),
                args: vec![],
                ret: i.void(),
                take_params: vec![matches!(case, 2 | 3)],
                borrowed_result: false,
            },
            args: vec![op(if case == 3 { alias } else { p })],
        });
        b.terminate(Terminator::Goto(end));
        b.switch_to(end);
        b.push(Statement::Release(op(p)));
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        assert_eq!(
            run(&mut f, &i, &mir.layouts, &effects),
            case == 0,
            "case={case}"
        );
        let counts = f
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .filter(|s| matches!(s, Statement::Retain(_) | Statement::Release(_)))
            .count();
        assert_eq!(counts, if case == 0 { 0 } else { 2 });
    }
}

#[test]
fn a_reentered_retain_cannot_match_one_release_after_the_loop() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("loop", i.void());
    let p = b.new_param(i.string(), None);
    let c = b.new_param(i.bool(), None);
    let head = b.new_block();
    let end = b.new_block();
    b.terminate(Terminator::Goto(head));
    b.switch_to(head);
    b.push(Statement::Retain(Operand::Copy(Place::Local(p))));
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: head,
        else_blk: end,
    });
    b.switch_to(end);
    b.push(Statement::Release(Operand::Copy(Place::Local(p))));
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    let mir = Mir {
        profile: dream_abi::profile::CompileProfile::Release,
        ..Mir::default()
    };
    let effects = ModRefTable::after_rc(&mir, &i);
    assert!(!run(&mut f, &i, &mir.layouts, &effects));
}
