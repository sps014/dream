use super::*;
use crate::{BinOp, Callee, Const, build::FunctionBuilder};
use dream_types::{DefKind, TypeCtx};

fn fixture(written_field: usize, shared: bool) -> (MirFunction, TypeInterner, Local, BlockId) {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Body", vec![]);
    if shared {
        ctx.interner.mark_shared_def(def);
    }
    let ty = ctx.interner.struct_ty(def, vec![]);
    let mut b = FunctionBuilder::new("pairs", ctx.interner.void());
    let source = b.new_param(ty, None);
    let alias = b.new_param(ty, None);
    let condition = b.new_param(ctx.interner.bool(), None);
    let value = b.new_temp(ctx.interner.double());
    let header = b.new_block();
    let body = b.new_block();
    let done = b.new_block();
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(condition)),
        then_blk: body,
        else_blk: done,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(value),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: source,
            field: 0,
        })),
    );
    b.assign(
        Place::Field {
            base: alias,
            field: written_field,
        },
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(value)),
            Operand::Const(Const::Float(1.0)),
        ),
    );
    b.terminate(Terminator::Goto(header));
    b.switch_to(done);
    b.terminate(Terminator::Return(None));
    (b.finish(), ctx.interner, value, body)
}

#[test]
fn disjoint_alias_store_allows_caching_after_the_first_iteration() {
    let (mut f, i, value, body) = fixture(1, false);
    let first = f.block(body).clone();
    let blocks = f.blocks.len();
    assert!(hoist(&mut f, &i, &BTreeSet::new(), &mut Default::default()));
    assert!(matches!(f.block(body).stmts[0], Statement::Nop));
    assert!(
        matches!(&f.blocks[blocks + 1].stmts[0], Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Field { .. }))) if *d == value)
    );
    assert_eq!(format!("{:?}", f.blocks[blocks + 1]), format!("{first:?}"));
    // Only the successful initial entry test reaches the peeled iteration; a
    // zero-trip loop reaches the original exit without dereferencing the source.
    assert!(f.blocks[blocks].stmts.is_empty());
    assert!(
        matches!(f.blocks[blocks].terminator, Terminator::If { then_blk, else_blk, .. } if then_blk == BlockId((blocks + 1) as u32) && else_blk == BlockId(3))
    );
    assert!(!hoist(
        &mut f,
        &i,
        &BTreeSet::new(),
        &mut Default::default()
    ));
}

#[test]
fn same_field_through_an_alias_and_shared_objects_refuse_caching() {
    for (field, shared) in [(0, false), (1, true)] {
        let (mut f, i, _, _) = fixture(field, shared);
        assert!(!hoist(
            &mut f,
            &i,
            &BTreeSet::new(),
            &mut Default::default()
        ));
    }
}

#[test]
fn unknown_call_release_and_raw_store_refuse_caching() {
    for kind in 0..4 {
        let (mut f, i, _, body) = fixture(1, false);
        let op = Operand::Copy(Place::Local(Local(0)));
        let effect = match kind {
            0 => Statement::Call {
                callee: Callee {
                    def: DefId::root(99),
                    args: vec![],
                    ret: i.void(),
                    take_params: vec![],
                },
                args: vec![],
            },
            1 => Statement::Release(op),
            2 => Statement::Assign(Place::Global(crate::Global(0)), Rvalue::Use(op)),
            _ => Statement::Assign(
                Place::Deref {
                    ptr: Local(0),
                    elem_ty: i.double(),
                },
                Rvalue::Use(Operand::Const(Const::Float(1.0))),
            ),
        };
        f.block_mut(body).stmts.push(effect);
        assert!(!hoist(
            &mut f,
            &i,
            &BTreeSet::new(),
            &mut Default::default()
        ));
    }
}

#[test]
fn source_reassignment_and_live_out_results_refuse_caching() {
    for live_out in [false, true] {
        let (mut f, i, value, body) = fixture(1, false);
        if live_out {
            f.blocks[3].terminator = Terminator::Return(Some(Operand::Copy(Place::Local(value))));
        } else {
            f.block_mut(body).stmts.push(Statement::Assign(
                Place::Local(Local(0)),
                Rvalue::Use(Operand::Copy(Place::Local(Local(1)))),
            ));
        }
        assert!(!hoist(
            &mut f,
            &i,
            &BTreeSet::new(),
            &mut Default::default()
        ));
    }
}
