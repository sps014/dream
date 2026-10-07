use super::*;
use crate::build::FunctionBuilder;
use crate::{Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefKind, TypeCtx};

#[test]
fn escaped_returns_are_rejected_without_removing_region_markers() {
    for nested in [false, true] {
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(def, vec![]);
        let mut f = FunctionBuilder::new("escaped", ty);
        let x = f.new_local(ty, None);
        f.push(Statement::RegionEnter);
        if nested {
            f.push(Statement::RegionEnter);
        }
        f.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
                policy: crate::AllocPolicy::Tracked,
            },
        );
        f.push(Statement::RegionLeave);
        if nested {
            f.push(Statement::RegionLeave);
        }
        f.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
        let mir = Mir {
            functions: vec![f.finish()],
            ..Default::default()
        };
        let before = format!("{:?}", mir.functions[0].blocks);
        let violations = verify_module(&mir, &ctx.interner);
        assert!(
            violations
                .iter()
                .any(|v| v.msg.contains("allocation region was left")),
            "{:?}",
            violations
        );
        assert_eq!(before, format!("{:?}", mir.functions[0].blocks));
    }
}
