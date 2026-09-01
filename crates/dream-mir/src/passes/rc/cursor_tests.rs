//! Cursor-candidacy and self-realloc interaction tests for [`super::RcInsertion`].
//!
//! A field/index snapshot whose source slot is overwritten must not stay a non-owning
//! cursor, and a snapshot's token must be dropped *before* a self-realloc consumes the
//! backing block.

use super::RcInsertion;
use crate::build::FunctionBuilder;
use crate::passes::MirPass;
use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefKind, TypeCtx};

fn string_list(ctx: &mut TypeCtx) -> dream_types::TypeId {
    ctx.interner.array(ctx.interner.string())
}

/// `let s = this.f;` … `this.f = <other>` — the reader must escape cursor candidacy.
#[test]
fn slot_overwrite_escapes_cursor_candidacy() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Opt", vec![]);
    let opt_ty = ctx.interner.struct_ty(def, vec![]);
    let list = ctx.interner.array(opt_ty);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let this = b.new_local(opt_ty, Some("self".into()));
    let other = b.new_local(list, Some("other".into()));
    let snap = b.new_local(list, Some("s".into()));
    b.assign(
        Place::Local(snap),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    b.assign(
        Place::Field {
            base: this,
            field: 0,
        },
        Rvalue::Use(Operand::Copy(Place::Local(other))),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(snap)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        !func.locals[snap.0 as usize].is_cursor,
        "snapshot of an overwritten slot must own its reference: {:?}",
        func.locals[snap.0 as usize]
    );
}

/// Same shape but the slot is never stored — the cursor optimization must survive.
#[test]
fn untouched_slot_stays_cursor() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Opt", vec![]);
    let opt_ty = ctx.interner.struct_ty(def, vec![]);
    let list = ctx.interner.array(opt_ty);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let this = b.new_local(opt_ty, Some("self".into()));
    let snap = b.new_local(list, Some("s".into()));
    b.assign(
        Place::Local(snap),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        func.locals[snap.0 as usize].is_cursor,
        "read-only snapshot should stay a non-owning cursor: {:?}",
        func.locals[snap.0 as usize]
    );
}

/// `let s = this.items;` then `this.items = Buffer.realloc(this.items, n)` — the snapshot's
/// token must be released before the realloc consumes the block.
#[test]
fn self_realloc_releases_snapshot_before_store() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "List", vec![]);
    let list_ty = ctx.interner.struct_ty(def, vec![]);
    let items = string_list(&mut ctx);
    let mut b = FunctionBuilder::new("grow", ctx.interner.void());
    let this = b.new_local(list_ty, Some("self".into()));
    let len = b.new_local(ctx.interner.int(), Some("old_cap".into()));
    let new_len = b.new_local(ctx.interner.int(), Some("new_cap".into()));
    let snap = b.new_local(items, Some("s".into()));
    b.assign(
        Place::Local(snap),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    b.assign(
        Place::Local(len),
        Rvalue::ArrayLen(Operand::Copy(Place::Local(snap))),
    );
    b.assign(
        Place::Local(new_len),
        Rvalue::Binary(
            crate::BinOp::Mul,
            Operand::Copy(Place::Local(len)),
            Operand::Const(Const::Int(2)),
        ),
    );
    b.assign(
        Place::Field {
            base: this,
            field: 0,
        },
        Rvalue::ArrayRealloc {
            elem_ty: ctx.interner.string(),
            array: Operand::Copy(Place::Field {
                base: this,
                field: 0,
            }),
            new_len: Operand::Copy(Place::Local(new_len)),
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);

    let stmts = &func.blocks[0].stmts;
    let realloc_pos = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(_, Rvalue::ArrayRealloc { .. })))
        .expect("realloc store survives RC insertion");
    let release_pos = stmts.iter().position(
        |s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l))) if *l == snap),
    );
    let release_pos =
        release_pos.unwrap_or_else(|| panic!("snapshot token must be released: {:?}", stmts));
    assert!(
        release_pos < realloc_pos,
        "snapshot release must precede the self-realloc (release at {}, realloc at {}): {:?}",
        release_pos,
        realloc_pos,
        stmts
    );
}
