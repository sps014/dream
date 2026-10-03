use super::ownership_args::run;
use crate::build::FunctionBuilder;
use crate::{Callee, Global, Mir, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeCtx};

#[test]
fn taken_globals_and_duplicate_projections_get_distinct_typed_tokens() {
    let ctx = TypeCtx::new();
    let ty = ctx.interner.string();
    let mut f = FunctionBuilder::new("takes", ctx.interner.void());
    f.push(Statement::Call {
        callee: Callee {
            def: DefId::root(1),
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true, true],
        },
        args: vec![Operand::Copy(Place::Global(Global(0))); 2],
    });
    f.terminate(Terminator::Return(None));
    let mut mir = Mir {
        functions: vec![f.finish()],
        ..Mir::default()
    };
    mir.globals.push(crate::MirGlobal { id: Global(0), ty });
    run(&mut mir, &ctx.interner);
    let f = &mir.functions[0];
    assert_eq!(f.locals.len(), 2);
    assert!(f.locals.iter().all(|l| l.ty == ty));
    assert!(matches!(
        f.blocks[0].stmts[0],
        Statement::Assign(
            Place::Local(_),
            Rvalue::Use(Operand::Copy(Place::Global(_)))
        )
    ));
    let Statement::Call { args, .. } = &f.blocks[0].stmts[2] else {
        panic!("normalized call")
    };
    assert!(
        matches!((&args[0], &args[1]), (Operand::Copy(Place::Local(a)), Operand::Copy(Place::Local(b))) if a != b)
    );
}

#[test]
fn borrowed_global_arguments_need_no_temporary() {
    let ctx = TypeCtx::new();
    let mut f = FunctionBuilder::new("discard", ctx.interner.void());
    f.push(Statement::Call {
        callee: Callee {
            def: DefId::root(1),
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Global(Global(0)))],
    });
    f.terminate(Terminator::Return(None));
    let mut mir = Mir {
        functions: vec![f.finish()],
        ..Mir::default()
    };
    run(&mut mir, &ctx.interner);
    let f = &mir.functions[0];
    assert!(f.locals.is_empty());
    assert!(matches!(f.blocks[0].stmts[0], Statement::Call { .. }));
}
