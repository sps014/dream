use super::*;
use crate::build::FunctionBuilder;
use crate::{Const, MirGlobal};

#[test]
fn unread_managed_global_stores_preserve_ownership_and_publication() {
    let mut types = TypeInterner::new();
    let scalar = types.int();
    let string = types.string();
    let inline = types.tuple_ty(vec![string, scalar]);
    let mut mir = Mir {
        globals: [scalar, string, inline, scalar]
            .into_iter()
            .enumerate()
            .map(|(id, ty)| MirGlobal {
                id: Global(id as u32),
                ty,
            })
            .collect(),
        ..Mir::default()
    };
    let mut builder = FunctionBuilder::new("store", types.void());
    builder.assign(
        Place::Global(Global(1)),
        Rvalue::Use(Operand::Const(Const::Str("owned".into()))),
    );
    let value = builder.new_param(inline, None);
    builder.assign(
        Place::Global(Global(2)),
        Rvalue::Use(Operand::Copy(Place::Local(value))),
    );
    builder.assign(
        Place::Global(Global(3)),
        Rvalue::Use(Operand::Const(Const::Int(7))),
    );
    builder.terminate(Terminator::Return(None));
    mir.functions.push(builder.finish());

    prune_dead_globals(&mut mir, &types);

    assert_eq!(
        mir.globals.iter().map(|g| g.id).collect::<Vec<_>>(),
        [Global(0), Global(1), Global(2)]
    );
    assert_eq!(mir.functions[0].blocks[0].stmts.len(), 2);
}
