use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeCtx;
use std::{cell::Cell, rc::Rc};

struct Counted {
    calls: Rc<Cell<usize>>,
    from: Option<i64>,
    to: i64,
}

impl MirPass for Counted {
    fn name(&self) -> &'static str {
        "counted-test"
    }
    fn preserves(&self) -> PreservedAnalyses {
        PreservedAnalyses::ControlFlow
    }
    fn transform(
        &self,
        function: &mut MirFunction,
        _: &TypeInterner,
        _: &dream_hir::LayoutTable,
        _: &mut FunctionAnalyses,
    ) -> bool {
        self.calls.set(self.calls.get() + 1);
        let Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Int(value)))) =
            &mut function.blocks[0].stmts[0]
        else {
            panic!("test constant")
        };
        if self.from == Some(*value) {
            *value = self.to;
            true
        } else {
            false
        }
    }
}

#[test]
fn unchanged_passes_skip_identical_inputs_but_revisit_new_opportunities() {
    let ctx = TypeCtx::new();
    let ty = ctx.interner.int();
    let mut builder = FunctionBuilder::new("revisioned", ty);
    let value = builder.new_local(ty, None);
    builder.assign(
        Place::Local(value),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    builder.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
    let mut function = builder.finish();
    let calls: Vec<_> = (0..3).map(|_| Rc::new(Cell::new(0))).collect();
    let mut pipeline = PassManager::new();
    pipeline.add(Counted {
        calls: calls[0].clone(),
        from: Some(2),
        to: 3,
    });
    pipeline.add(Counted {
        calls: calls[1].clone(),
        from: Some(0),
        to: 2,
    });
    pipeline.add(Counted {
        calls: calls[2].clone(),
        from: None,
        to: 0,
    });
    pipeline.run(&mut function, &ctx.interner);
    assert_eq!(calls.iter().map(|c| c.get()).collect::<Vec<_>>(), [3, 2, 2]);
    assert!(matches!(
        function.blocks[0].stmts[0],
        Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Int(3))))
    ));
    assert!(crate::verify::verify_function(&function, &ctx.interner).is_empty());
}

struct Toggle;

impl MirPass for Toggle {
    fn preserves(&self) -> crate::passes::PreservedAnalyses {
        crate::passes::PreservedAnalyses::ControlFlow
    }

    fn name(&self) -> &'static str {
        "toggle-test"
    }

    fn transform(
        &self,
        function: &mut MirFunction,
        _: &TypeInterner,
        _layouts: &dream_hir::LayoutTable,
        _analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
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
