//! Cursor-candidacy and self-realloc interaction tests for [`super::RcInsertion`].
//!
//! A field/index snapshot whose source slot is overwritten must not stay a non-owning
//! cursor, and a snapshot's token must be dropped *before* a self-realloc consumes the
//! backing block.

use super::RcInsertion;
use crate::build::FunctionBuilder;
use crate::passes::MirPass;
use crate::{Callee, Const, Operand, Place, Rvalue, Statement, Terminator};
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

/// Lowering writes `s = null` before `s = this.f`. That must not count as a second def.
#[test]
fn null_then_field_load_stays_cursor() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Opt", vec![]);
    let opt_ty = ctx.interner.struct_ty(def, vec![]);
    let list = ctx.interner.array(opt_ty);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let this = b.new_local(opt_ty, Some("self".into()));
    let snap = b.new_local(list, Some("s".into()));
    b.assign(Place::Local(snap), Rvalue::Use(Operand::Const(Const::Null)));
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
        "null-init then field load must stay a cursor: {:?}",
        func.locals[snap.0 as usize]
    );
}

/// `get` lowering: `this_2 = this;` (borrow param) then `m = null; m = this_2.obj_map`.
#[test]
fn borrow_this_copy_then_field_stays_cursor() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Json", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let map = ctx.interner.array(ctx.interner.string());
    let mut b = FunctionBuilder::new("get", ctx.interner.void());
    let this = b.new_param(ty, Some("this".into()));
    let this2 = b.new_local(ty, Some("this_2".into()));
    let snap = b.new_local(map, Some("m".into()));
    b.assign(
        Place::Local(this2),
        Rvalue::Use(Operand::Copy(Place::Local(this))),
    );
    b.assign(Place::Local(snap), Rvalue::Use(Operand::Const(Const::Null)));
    b.assign(
        Place::Local(snap),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this2,
            field: 0,
        })),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        func.locals[snap.0 as usize].is_cursor,
        "obj_map snapshot must stay a cursor: this2={:?} snap={:?} stmts={:?}",
        func.locals[this2.0 as usize], func.locals[snap.0 as usize], func.blocks[0].stmts
    );
}

/// Last-use copy of a borrow-field snapshot must stay a cursor (`m = this.obj_map`).
#[test]
fn last_use_copy_of_borrow_field_stays_cursor() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Json", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let map = ctx.interner.array(ctx.interner.string());
    let mut b = FunctionBuilder::new("get", ctx.interner.void());
    let this = b.new_param(ty, Some("this".into()));
    let this2 = b.new_local(ty, Some("this_2".into()));
    let snap = b.new_local(map, Some("l7".into()));
    let alias = b.new_local(map, Some("m".into()));
    b.assign(
        Place::Local(this2),
        Rvalue::Use(Operand::Copy(Place::Local(this))),
    );
    b.assign(
        Place::Local(snap),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this2,
            field: 0,
        })),
    );
    b.assign(
        Place::Local(alias),
        Rvalue::Use(Operand::Copy(Place::Local(snap))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        func.locals[snap.0 as usize].is_cursor && func.locals[alias.0 as usize].is_cursor,
        "l7={} m={} stmts={:?}",
        func.locals[snap.0 as usize].is_cursor,
        func.locals[alias.0 as usize].is_cursor,
        func.blocks[0].stmts
    );
}

/// `let alias = n; let arr = [n, alias]` — the copy is stored, so it must own a retain.
#[test]
fn forwarding_copy_stored_in_array_owns() {
    let mut ctx = TypeCtx::new();
    let (def, ty) = {
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        (def, ctx.interner.struct_ty(def, vec![]))
    };
    let arr_ty = ctx.interner.array(ty);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let n = b.new_local(ty, Some("n".into()));
    let alias = b.new_local(ty, Some("alias".into()));
    let arr = b.new_local(arr_ty, Some("arr".into()));
    b.assign(
        Place::Local(n),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(alias),
        Rvalue::Use(Operand::Copy(Place::Local(n))),
    );
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayLit {
            elem_ty: ty,
            elems: vec![
                Operand::Copy(Place::Local(n)),
                Operand::Copy(Place::Local(alias)),
            ],
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        !func.locals[alias.0 as usize].is_cursor,
        "stored forwarding copy must own: {:?}",
        func.locals[alias.0 as usize]
    );
}

/// `switch (this.obj_map) { Some(m) => … }` — niche payload is a UnionField of a borrow slot.
#[test]
fn union_field_of_borrow_slot_stays_cursor() {
    let mut ctx = TypeCtx::new();
    let jdef = ctx.register(DefKind::Struct, "Json", vec![]);
    let jty = ctx.interner.struct_ty(jdef, vec![]);
    let udef = ctx.register(DefKind::Union, "Opt", vec![]);
    let uty = ctx.interner.union_ty(udef, vec![jty]);
    let mut b = FunctionBuilder::new("get", ctx.interner.void());
    let this = b.new_param(jty, Some("this".into()));
    let slot = b.new_local(uty, Some("l7".into()));
    let m = b.new_local(jty, Some("m".into()));
    b.assign(
        Place::Local(slot),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    b.assign(
        Place::Local(m),
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(slot)),
            ty: uty,
            variant: 0,
            field: 0,
        },
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        func.locals[m.0 as usize].is_cursor,
        "Some(m) of this.obj_map must stay a cursor: {:?}",
        func.blocks[0].stmts
    );
}

/// `slots[i].value` must own: leftover last-ref of a cursor would free the Map occupant.
#[test]
fn field_of_index_load_owns() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "Json", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let slot_def = ctx.register(DefKind::Struct, "Entry", vec![]);
    let slot_ty = ctx.interner.struct_ty(slot_def, vec![]);
    let arr = ctx.interner.array(slot_ty);
    let mut b = FunctionBuilder::new("get", ctx.interner.void());
    let this = b.new_param(ty, Some("this".into()));
    let slots = b.new_local(arr, Some("slots".into()));
    let entry = b.new_local(slot_ty, Some("entry".into()));
    let val = b.new_local(ty, Some("value".into()));
    b.assign(
        Place::Local(slots),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: this,
            field: 0,
        })),
    );
    b.assign(
        Place::Local(entry),
        Rvalue::Use(Operand::Copy(Place::Index {
            base: slots,
            index: Box::new(Operand::Const(Const::Int(0))),
            unchecked: true,
        })),
    );
    b.assign(
        Place::Local(val),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: entry,
            field: 0,
        })),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        func.locals[slots.0 as usize].is_cursor,
        "this.slots field snapshot stays a cursor: {:?}",
        func.locals[slots.0 as usize]
    );
    assert!(
        !func.locals[entry.0 as usize].is_cursor && !func.locals[val.0 as usize].is_cursor,
        "index occupant and its field must own: entry={} val={} {:?}",
        func.locals[entry.0 as usize].is_cursor,
        func.locals[val.0 as usize].is_cursor,
        func.blocks[0].stmts
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

/// `let fname = field.name; take(field); use(fname)` — fname must own, not cursor, or last-use
/// destroy of `field` frees the string under the alias.
#[test]
fn cursor_escapes_when_base_dies_before_last_use() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "User", vec![]);
    let ty = ctx.interner.struct_ty(def, vec![]);
    let str_ty = ctx.interner.string();
    let take = ctx.register(DefKind::Function, "take", vec![]);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let field = b.new_local(ty, Some("field".into()));
    let fname = b.new_local(str_ty, Some("fname".into()));
    b.assign(
        Place::Local(field),
        Rvalue::New {
            def,
            ty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(fname),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: field,
            field: 0,
        })),
    );
    b.push(Statement::Call {
        callee: Callee {
            def: take,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![true],
        },
        args: vec![Operand::Copy(Place::Local(field))],
    });
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(fname))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    assert!(
        !func.locals[fname.0 as usize].is_cursor,
        "fname must own after base last-use: {:?}",
        func.locals[fname.0 as usize]
    );
    let has_retain = func.blocks[0]
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == fname));
    assert!(
        has_retain,
        "field-load that outlives the holder must Retain: {:?}",
        func.blocks[0].stmts
    );
}

/// `switch (res) { Ok(root) => use(root) }` — payload must own, or last-use destroy of `res`
/// frees the object under the arm binding.
#[test]
fn union_payload_escapes_when_scrutinee_dies() {
    let mut ctx = TypeCtx::new();
    let cty = {
        let def = ctx.register(DefKind::Struct, "Json", vec![]);
        ctx.interner.struct_ty(def, vec![])
    };
    let udef = ctx.register(DefKind::Union, "Result", vec![]);
    let uty = ctx.interner.union_ty(udef, vec![cty]);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let res = b.new_local(uty, Some("res".into()));
    let root = b.new_local(cty, Some("root".into()));
    b.assign(
        Place::Local(res),
        Rvalue::UnionNew {
            def: udef,
            ty: uty,
            variant: 0,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(root),
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(res)),
            ty: uty,
            variant: 0,
            field: 0,
        },
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(root))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let stmts = &func.blocks[0].stmts;
    let peek_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Call { callee, .. } if callee.def == peek));
    let drop_res = stmts.iter().position(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))
                | Statement::ReleaseUnique(Operand::Copy(Place::Local(l)))
            if *l == res
        )
    });
    let peek_at = peek_at.expect("peek call");
    let drop_res = drop_res.expect("res must still be destroyed");
    let owns = !func.locals[root.0 as usize].is_cursor;
    let has_retain = stmts
        .iter()
        .any(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == root));
    assert!(
        (owns && has_retain) || drop_res > peek_at,
        "payload must own or Result must live until peek: cursor={} retain={} drop={} peek={} {:?}",
        func.locals[root.0 as usize].is_cursor,
        has_retain,
        drop_res,
        peek_at,
        stmts
    );
}

/// Result payload loaded as a field cursor: last-use destroy of `res` must not run before `peek`.
#[test]
fn field_cursor_keeps_base_alive_until_last_use() {
    let mut ctx = TypeCtx::new();
    let cdef = ctx.register(DefKind::Struct, "Json", vec![]);
    let cty = ctx.interner.struct_ty(cdef, vec![]);
    let rdef = ctx.register(DefKind::Struct, "Result", vec![]);
    let rty = ctx.interner.struct_ty(rdef, vec![]);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let res = b.new_local(rty, Some("res".into()));
    let root = b.new_local(cty, Some("root".into()));
    b.assign(
        Place::Local(res),
        Rvalue::New {
            def: rdef,
            ty: rty,
            ctor: None,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(root),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: res,
            field: 0,
        })),
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(root))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    let stmts = &func.blocks[0].stmts;
    let peek_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Call { callee, .. } if callee.def == peek));
    let drop_res = stmts.iter().position(|s| {
        matches!(
            s,
            Statement::Release(Operand::Copy(Place::Local(l)))
                | Statement::ReleaseUnique(Operand::Copy(Place::Local(l)))
            if *l == res
        )
    });
    let peek_at = peek_at.expect("peek call");
    let drop_res = drop_res.expect("res must still be destroyed");
    let has_retain = stmts
        .iter()
        .any(|s| matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == root));
    assert!(
        has_retain || drop_res > peek_at,
        "field snapshot must Retain or keep Result alive until peek: {:?}",
        stmts
    );
}

/// `x = t[0]` then `arr[0] = w` — temps for the same array still invalidate the snapshot.
#[test]
fn index_store_escapes_load_through_copy() {
    let mut ctx = TypeCtx::new();
    let def = ctx.register(DefKind::Struct, "W", vec![]);
    let wty = ctx.interner.struct_ty(def, vec![]);
    let arr_ty = ctx.interner.array(wty);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let arr = b.new_local(arr_ty, Some("arr".into()));
    let tmp = b.new_local(arr_ty, Some("tmp".into()));
    let w = b.new_local(wty, Some("w".into()));
    let x = b.new_local(wty, Some("x".into()));
    b.assign(
        Place::Local(tmp),
        Rvalue::Use(Operand::Copy(Place::Local(arr))),
    );
    b.assign(
        Place::Local(x),
        Rvalue::Use(Operand::Copy(Place::Index {
            base: tmp,
            index: Box::new(Operand::Const(Const::Int(0))),
            unchecked: true,
        })),
    );
    b.assign(
        Place::Index {
            base: arr,
            index: Box::new(Operand::Const(Const::Int(0))),
            unchecked: true,
        },
        Rvalue::Use(Operand::Copy(Place::Local(w))),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &ctx.interner);
    assert!(
        !func.locals[x.0 as usize].is_cursor,
        "index snapshot must own across a store to another name for the array: {:?}",
        func.locals[x.0 as usize]
    );
}

/// Match payload used after the last read of the union, including a hop that rebinds the union.
#[test]
fn union_hop_payload_is_not_cursor() {
    let mut ctx = TypeCtx::new();
    let cty = {
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        ctx.interner.struct_ty(def, vec![])
    };
    let udef = ctx.register(DefKind::Union, "Opt", vec![]);
    let uty = ctx.interner.union_ty(udef, vec![cty]);
    let peek = ctx.register(DefKind::Function, "peek", vec![]);
    let mut b = FunctionBuilder::new("f", ctx.interner.void());
    let curr = b.new_local(uty, Some("curr".into()));
    let n = b.new_local(cty, Some("n".into()));
    b.assign(
        Place::Local(curr),
        Rvalue::UnionNew {
            def: udef,
            ty: uty,
            variant: 0,
            args: vec![],
        },
    );
    b.assign(
        Place::Local(n),
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(curr)),
            ty: uty,
            variant: 0,
            field: 0,
        },
    );
    b.assign(
        Place::Local(curr),
        Rvalue::Use(Operand::Copy(Place::Field { base: n, field: 0 })),
    );
    b.push(Statement::Call {
        callee: Callee {
            def: peek,
            args: vec![],
            ret: ctx.interner.void(),
            take_params: vec![false],
        },
        args: vec![Operand::Copy(Place::Local(curr))],
    });
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &ctx.interner));
    assert!(
        !func.locals[n.0 as usize].is_cursor,
        "hop payload must own or unique-destroy of curr frees it: {:?}",
        func.locals[n.0 as usize]
    );
}
