use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeCtx;

struct Toggle;

impl MirPass for Toggle {
    fn name(&self) -> &'static str {
        "toggle-test"
    }

    fn run(&self, function: &mut MirFunction, _: &TypeInterner) -> bool {
        let Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Int(value)))) =
            &mut function.blocks[0].stmts[0]
        else {
            panic!("test constant")
        };
        *value ^= 1;
        true
    }
}

#[test]
fn cap_counts_only_a_changed_final_round() {
    let ctx = TypeCtx::new();
    let ty = ctx.interner.int();
    let mut builder = FunctionBuilder::new("bounded", ty);
    let value = builder.new_local(ty, None);
    builder.assign(
        Place::Local(value),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    builder.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
    let mut function = builder.finish();
    let before = limits::hits(limits::Limit::Function);
    let mut pipeline = PassManager::new();
    pipeline.max_iterations = 2;
    pipeline.add(Toggle);
    pipeline.run(&mut function, &ctx.interner);
    assert_eq!(limits::hits(limits::Limit::Function), before + 1);
    assert!(crate::verify::verify_function(&function, &ctx.interner).is_empty());
    let mut converging = PassManager::new();
    converging.max_iterations = 1;
    converging.add(ConstFold);
    converging.run(&mut function, &ctx.interner);
    assert_eq!(limits::hits(limits::Limit::Function), before + 1);
}
