use super::*;
use crate::build::FunctionBuilder;
use dream_types::{DefKind, TypeCtx};

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "escaped inferred region in escaped")]
fn debug_pipeline_rejects_instead_of_silently_repairing() {
    let (ctx, mut mir) = escaped_module();
    strip_escaped_regions(&mut mir, &ctx.interner);
}

#[cfg(not(debug_assertions))]
#[test]
fn release_pipeline_removes_escaped_regions() {
    let (ctx, mut mir) = escaped_module();
    assert!(strip_escaped_regions(&mut mir, &ctx.interner));
    assert!(mir.functions[0].blocks.iter().all(|b| b
        .stmts
        .iter()
        .all(|s| !matches!(s, Statement::RegionEnter | Statement::RegionLeave))));
    assert!(crate::verify::verify_module(&mir, &ctx.interner).is_empty());
}

#[test]
fn nested_scopes_are_removed_as_a_complete_cfg_set() {
    let (ctx, mut mir) = escaped_module();
    let f = &mut mir.functions[0];
    f.blocks[0].stmts.insert(0, Statement::RegionEnter);
    f.blocks[0].stmts.push(Statement::RegionLeave);
    assert!(strip_escaped_fn(f, &ctx.interner));
    assert!(f.blocks.iter().all(|b| b
        .stmts
        .iter()
        .all(|s| !matches!(s, Statement::RegionEnter | Statement::RegionLeave))));
}

fn escaped_module() -> (TypeCtx, Mir) {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Node", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut f = FunctionBuilder::new("escaped", ty);
    let x = f.new_local(ty, None);
    f.push(Statement::RegionEnter);
    f.assign(
        Place::Local(x),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    f.push(Statement::RegionLeave);
    f.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
    (
        ctx,
        Mir {
            functions: vec![f.finish()],
            ..Mir::default()
        },
    )
}
