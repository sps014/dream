use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, Operand, Statement, Terminator};

fn diamond() -> MirFunction {
    let types = TypeInterner::new();
    let mut b = FunctionBuilder::new("diamond", types.void());
    let left = b.new_block();
    let right = b.new_block();
    let join = b.new_block();
    b.terminate(Terminator::If {
        cond: Operand::Const(Const::Bool(true)),
        then_blk: left,
        else_blk: right,
    });
    b.switch_to(left);
    b.terminate(Terminator::Goto(join));
    b.switch_to(right);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.terminate(Terminator::Return(None));
    b.finish()
}

#[test]
fn dependent_queries_share_predecessors_and_dominators() {
    let func = diamond();
    let mut analyses = FunctionAnalyses::default();
    let preds = analyses.predecessors(&func);
    let rpo = analyses.reverse_postorder(&func);
    let dom = analyses.dominators(&func);
    let loops = analyses.natural_loops(&func);
    assert!(Rc::ptr_eq(&preds, &analyses.predecessors(&func)));
    assert!(Rc::ptr_eq(&rpo, &analyses.reverse_postorder(&func)));
    assert!(Rc::ptr_eq(&dom, &analyses.dominators(&func)));
    assert!(Rc::ptr_eq(&loops, &analyses.natural_loops(&func)));
    assert_eq!(preds[3], [BlockId(1), BlockId(2)]);
    assert_eq!(dom.idom(BlockId(3)), Some(BlockId(0)));
}

struct Edit {
    cfg: bool,
    preserves: PreservedAnalyses,
}

struct SilentCfgEdit;

impl MirPass for SilentCfgEdit {
    fn name(&self) -> &'static str {
        "silent-edit-test"
    }

    fn preserves(&self) -> PreservedAnalyses {
        PreservedAnalyses::None
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        _: &TypeInterner,
        _: &LayoutTable,
        _: &mut FunctionAnalyses,
    ) -> bool {
        func.blocks[0].terminator = Terminator::Goto(BlockId(1));
        false
    }
}

#[test]
#[should_panic(expected = "changed control flow but reported no change")]
fn unreported_cfg_edit_is_an_ice() {
    SilentCfgEdit.run(&mut diamond(), &TypeInterner::new());
}

impl MirPass for Edit {
    fn name(&self) -> &'static str {
        "edit-test"
    }

    fn preserves(&self) -> PreservedAnalyses {
        self.preserves
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        _: &TypeInterner,
        _: &LayoutTable,
        _: &mut FunctionAnalyses,
    ) -> bool {
        if self.cfg {
            func.blocks[0].terminator = Terminator::Goto(BlockId(1));
        } else {
            func.blocks[0].stmts.push(Statement::Nop);
        }
        true
    }
}

#[test]
fn statement_edits_preserve_cached_results_and_cfg_edits_rebuild_them() {
    let types = TypeInterner::new();
    let mut func = diamond();
    let mut analyses = FunctionAnalyses::default();
    let preds = analyses.predecessors(&func);
    let dom = analyses.dominators(&func);
    let pdom = analyses.postdominators(&func);
    let loops = analyses.natural_loops(&func);
    let layouts = LayoutTable::default();
    analyses.run_pass(
        &Edit {
            cfg: false,
            preserves: PreservedAnalyses::ControlFlow,
        },
        &mut func,
        &types,
        &layouts,
    );
    assert!(Rc::ptr_eq(&preds, &analyses.predecessors(&func)));
    assert!(Rc::ptr_eq(&dom, &analyses.dominators(&func)));
    assert!(Rc::ptr_eq(&pdom, &analyses.postdominators(&func)));
    assert!(Rc::ptr_eq(&loops, &analyses.natural_loops(&func)));
    analyses.run_pass(
        &Edit {
            cfg: true,
            preserves: PreservedAnalyses::None,
        },
        &mut func,
        &types,
        &layouts,
    );
    assert!(!Rc::ptr_eq(&preds, &analyses.predecessors(&func)));
    let rebuilt = analyses.dominators(&func);
    assert!(!Rc::ptr_eq(&dom, &rebuilt));
    assert_eq!(rebuilt.idom(BlockId(3)), Some(BlockId(1)));
    assert!(analyses.predecessors(&func)[2].is_empty());
    assert!(!Rc::ptr_eq(&pdom, &analyses.postdominators(&func)));
    assert!(!Rc::ptr_eq(&loops, &analyses.natural_loops(&func)));
}

#[test]
#[should_panic(expected = "violated its control-flow preservation contract")]
fn incorrect_preservation_is_an_ice() {
    let types = TypeInterner::new();
    Edit {
        cfg: true,
        preserves: PreservedAnalyses::ControlFlow,
    }
    .run(&mut diamond(), &types);
}

struct Observe(std::rc::Rc<std::cell::RefCell<Vec<Rc<DomTree>>>>);

impl MirPass for Observe {
    fn name(&self) -> &'static str {
        "observe-test"
    }

    fn preserves(&self) -> PreservedAnalyses {
        PreservedAnalyses::None
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        _: &TypeInterner,
        _: &LayoutTable,
        analyses: &mut FunctionAnalyses,
    ) -> bool {
        self.0.borrow_mut().push(analyses.dominators(func));
        false
    }
}

struct AppendOnce;

impl MirPass for AppendOnce {
    fn name(&self) -> &'static str {
        "append-once-test"
    }

    fn preserves(&self) -> PreservedAnalyses {
        PreservedAnalyses::ControlFlow
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        _: &TypeInterner,
        _: &LayoutTable,
        _: &mut FunctionAnalyses,
    ) -> bool {
        if func.blocks[0].stmts.is_empty() {
            func.blocks[0].stmts.push(Statement::Nop);
            true
        } else {
            false
        }
    }
}

#[test]
fn pipeline_shares_across_passes_and_rounds_but_never_across_functions() {
    let types = TypeInterner::new();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut pipeline = crate::passes::PassManager::new();
    pipeline.add(Observe(seen.clone()));
    pipeline.add(AppendOnce);
    pipeline.add(Observe(seen.clone()));
    pipeline.run(&mut diamond(), &types);
    let first_function = seen.borrow().len();
    assert!(first_function > 2, "the changed round must rerun");
    pipeline.run(&mut diamond(), &types);
    let seen = seen.borrow();
    let (first, second) = seen.split_at(first_function);
    assert!(first.iter().all(|dom| Rc::ptr_eq(dom, &first[0])));
    assert!(second.iter().all(|dom| Rc::ptr_eq(dom, &second[0])));
    assert!(!Rc::ptr_eq(&first[0], &second[0]));
}

#[test]
fn entry_and_block_identity_changes_invalidate_all_dependencies() {
    let mut func = diamond();
    let mut analyses = FunctionAnalyses::default();
    let dom = analyses.dominators(&func);
    let rpo = analyses.reverse_postorder(&func);
    func.entry = BlockId(1);
    analyses.invalidate();
    assert_eq!(analyses.reverse_postorder(&func).first(), Some(&BlockId(1)));
    assert!(!Rc::ptr_eq(&dom, &analyses.dominators(&func)));
    assert!(!Rc::ptr_eq(&rpo, &analyses.reverse_postorder(&func)));
    let preds = analyses.predecessors(&func);
    func.blocks.push(crate::BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Return(None),
    });
    analyses.invalidate();
    assert_eq!(analyses.predecessors(&func).len(), func.blocks.len());
    assert!(!Rc::ptr_eq(&preds, &analyses.predecessors(&func)));
}
