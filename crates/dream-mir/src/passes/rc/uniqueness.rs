//! Unique vs Shared lattice for owned RC locals.
//!
//! Empty / Unique / Shared is a join-semilattice on top of ownership tokens. Unique means this
//! local is the only live +1; Shared means a copy escaped or a still-live alias exists. Never
//! under-retain: unproven uniqueness is Shared.

use super::liveness::{live_after_stmt, stmt_reads_local};
use super::tokens::move_source;
use super::{is_borrowed_copy, rvalue_reads_local};
use crate::{Const, MirFunction, Operand, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::HashSet;

/// Last-use store of an owned local into a field, index, or global transfers the +1.
pub(crate) fn collect_container_moves(
    func: &MirFunction,
    interner: &TypeInterner,
    live_out: &[HashSet<u32>],
    is_owned: impl Fn(u32) -> bool,
    layouts: &LayoutTable,
    sink_move: &mut HashSet<(usize, usize, u32)>,
) {
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            for src in container_move_locals(stmt) {
                if !is_owned(src) || !can_container_move(interner, func.locals[src as usize].ty) {
                    continue;
                }
                if field_store_is_non_strong(func, layouts, stmt) {
                    continue;
                }
                if stmt_local_read_count(stmt, src) != 1 {
                    continue;
                }
                if live_after_stmt(func, live_out, bi, si, src) {
                    continue;
                }
                sink_move.insert((bi, si, src));
            }
        }
    }
}

pub(crate) fn container_move_locals(stmt: &Statement) -> Vec<u32> {
    if let Some(src) = container_store_src(stmt) {
        vec![src]
    } else {
        Vec::new()
    }
}

/// Payload locals of `UnionNew` / array / tuple construction. These are marked Shared (callee-style
/// retain in the emitter) but are not last-use moves: moving would drop the source token before
/// end-of-scope, which breaks `weak` / `unowned` observers.
pub(crate) fn constructed_payload_locals(stmt: &Statement) -> Vec<u32> {
    let mut out = Vec::new();
    if let Statement::Assign(
        _,
        Rvalue::New {
            ctor: None, args, ..
        }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. },
    ) = stmt
    {
        for a in args {
            if let Operand::Copy(Place::Local(l)) = a {
                out.push(l.0);
            }
        }
    }
    out
}

pub(crate) fn container_store_src(stmt: &Statement) -> Option<u32> {
    match stmt {
        Statement::Assign(
            Place::Field { .. } | Place::Index { .. } | Place::Global(_),
            Rvalue::Use(Operand::Copy(Place::Local(src)))
            | Rvalue::Cast(Operand::Copy(Place::Local(src)), _, _),
        ) => Some(src.0),
        // Already settled as a handoff; still a container store to everything that asks.
        Statement::Assign(_, Rvalue::Move { src, .. }) => Some(src.0),
        _ => None,
    }
}

/// For each local, whether every value it is ever given is freshly allocated — so it holds the only
/// reference to its object, and a container store reading it adopts that `+1` instead of taking its
/// own reference.
///
/// This is what separates a real handoff from a store that merely *looks* like one. A local copied
/// from a parameter, from a string literal, or out of another container owns nothing, so a store
/// reading it has to retain; marking such a store as a move drops the object's only owner. Answering
/// it per local and over the whole function also covers a `switch` result, which is assigned in each
/// arm and reaches the store from a predecessor block. Nulls are skipped: they end a local's life
/// rather than give it a value.
pub(crate) fn fresh_locals(func: &MirFunction, interner: &TypeInterner) -> Vec<bool> {
    #[derive(Clone, Copy)]
    enum Def {
        Alloc,
        /// A copy or cast of another local, which is fresh exactly when its source is.
        From(u32),
        Borrow,
    }
    let n = func.locals.len();
    let mut defs: Vec<Vec<Def>> = vec![Vec::new(); n];
    for stmt in func.blocks.iter().flat_map(|b| &b.stmts) {
        let Statement::Assign(Place::Local(d), rv) = stmt else {
            continue;
        };
        if d.0 as usize >= n || matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) {
            continue;
        }
        defs[d.0 as usize].push(if rvalue_allocates(interner, rv) {
            Def::Alloc
        } else {
            match rv {
                Rvalue::Use(Operand::Copy(Place::Local(s)))
                | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _) => Def::From(s.0),
                _ => Def::Borrow,
            }
        });
    }
    // Optimistic, then narrowed to a fixpoint, so a chain of copies off one allocation stays fresh
    // while a single borrowing definition anywhere in the chain disqualifies the whole chain.
    let mut fresh: Vec<bool> = defs
        .iter()
        .map(|d| !d.is_empty() && !d.iter().any(|k| matches!(k, Def::Borrow)))
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for l in 0..n {
            if !fresh[l] {
                continue;
            }
            if defs[l]
                .iter()
                .any(|k| matches!(k, Def::From(s) if !fresh[*s as usize]))
            {
                fresh[l] = false;
                changed = true;
            }
        }
    }
    fresh
}

/// Whether the rvalue yields a fresh `+1` rather than a reference someone else still owns. A niche
/// union is a pointer pun over its payload, not an allocation, so it hands over nothing of its own.
fn rvalue_allocates(interner: &TypeInterner, rv: &Rvalue) -> bool {
    match rv {
        Rvalue::UnionNew { ty, .. } => !interner.is_niche_union(*ty),
        Rvalue::Call { .. }
        | Rvalue::IndirectCall { .. }
        | Rvalue::InterfaceCall { .. }
        | Rvalue::New { .. }
        | Rvalue::ArrayNew { .. }
        | Rvalue::ArrayLit { .. }
        | Rvalue::Tuple { .. }
        | Rvalue::Concat(_)
        | Rvalue::ConcatInt { .. }
        | Rvalue::ToString(_) => true,
        _ => false,
    }
}

/// Records in the store itself that it adopts `src`'s token, so the C backend reads the transfer off
/// the statement rather than re-deriving it from the `src = null` that follows. The null stays: it
/// keeps the dead pointer out of async frame spills and later reads, but it no longer carries the
/// ownership meaning by itself.
pub(crate) fn mark_container_move(stmt: &mut Statement, src_local: u32) {
    let Statement::Assign(place, rvalue) = stmt else {
        return;
    };
    if !matches!(
        place,
        Place::Field { .. } | Place::Index { .. } | Place::Global(_)
    ) {
        return;
    }
    let (src, cast) = match &*rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) => (*src, None),
        Rvalue::Cast(Operand::Copy(Place::Local(src)), from, to) => (*src, Some((*from, *to))),
        _ => return,
    };
    if src.0 == src_local {
        *rvalue = Rvalue::Move { src, cast };
    }
}

pub(crate) fn field_store_is_non_strong(
    func: &MirFunction,
    layouts: &LayoutTable,
    stmt: &Statement,
) -> bool {
    let Statement::Assign(Place::Field { base, field }, _) = stmt else {
        return false;
    };
    layouts
        .get(func.local_ty(*base))
        .and_then(|layout| layout.fields.get(*field))
        .is_some_and(|f| f.is_weak || f.is_unowned)
}

pub(crate) fn stmt_local_read_count(stmt: &Statement, local: u32) -> u32 {
    if !stmt_reads_local(stmt, local) {
        return 0;
    }
    match stmt {
        Statement::Assign(place, rv) => {
            place_local_reads(place, local) + rvalue_local_reads(rv, local)
        }
        _ => {
            if stmt_reads_local(stmt, local) {
                1
            } else {
                0
            }
        }
    }
}

fn place_local_reads(place: &Place, local: u32) -> u32 {
    match place {
        Place::Local(l) | Place::Field { base: l, .. } | Place::Deref { ptr: l, .. } => {
            u32::from(l.0 == local)
        }
        Place::Index { base, index, .. } => {
            u32::from(base.0 == local) + operand_local_reads(index, local)
        }
        Place::Global(_) => 0,
    }
}

fn operand_local_reads(op: &Operand, local: u32) -> u32 {
    match op {
        Operand::Copy(p) => place_local_reads(p, local),
        Operand::Const(_) => 0,
    }
}

fn rvalue_local_reads(rv: &Rvalue, local: u32) -> u32 {
    match rv {
        Rvalue::Move { src, .. } => u32::from(src.0 == local),
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::CheckedNeg(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::StrBytes(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::TypeName(o)
        | Rvalue::Discriminant { base: o, .. }
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. } => operand_local_reads(o, local),
        Rvalue::Binary(_, a, b)
        | Rvalue::CheckedBinary(_, a, b)
        | Rvalue::CharAt(a, b, _)
        | Rvalue::ByteAt(a, b, _)
        | Rvalue::LoadU8(a, b)
        | Rvalue::LoadU16(a, b) => operand_local_reads(a, local) + operand_local_reads(b, local),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            operand_local_reads(cond, local)
                + operand_local_reads(then_val, local)
                + operand_local_reads(else_val, local)
        }
        Rvalue::Concat(parts) => parts.iter().map(|p| operand_local_reads(p, local)).sum(),
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => {
            operand_local_reads(prefix, local)
                + operand_local_reads(value, local)
                + operand_local_reads(suffix, local)
        }
        Rvalue::EnumName { value, .. } => operand_local_reads(value, local),
        Rvalue::ArrayNew { len, .. } => operand_local_reads(len, local),
        Rvalue::ToBytes { value: o, .. } | Rvalue::FromBytes { bytes: o, .. } => {
            operand_local_reads(o, local)
        }
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            operand_local_reads(array, local) + operand_local_reads(new_len, local)
        }
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => {
            args.iter().map(|a| operand_local_reads(a, local)).sum()
        }
        Rvalue::IndirectCall { target, args, .. } => {
            operand_local_reads(target, local)
                + args
                    .iter()
                    .map(|a| operand_local_reads(a, local))
                    .sum::<u32>()
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            operand_local_reads(receiver, local)
                + args
                    .iter()
                    .map(|a| operand_local_reads(a, local))
                    .sum::<u32>()
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            operand_local_reads(target, local)
                + via
                    .as_ref()
                    .map(|v| operand_local_reads(v, local))
                    .unwrap_or(0)
                + method
                    .as_ref()
                    .map(|m| operand_local_reads(m, local))
                    .unwrap_or(0)
                + args
                    .iter()
                    .map(|(a, _)| operand_local_reads(a, local))
                    .sum::<u32>()
        }
        Rvalue::FuncRef(_) => 0,
    }
}

/// Last-use store may transfer +1 for any owned RC local except `js` / `@shared`.
pub(crate) fn can_container_move(interner: &TypeInterner, ty: TypeId) -> bool {
    interner.is_rc_tracked(ty)
        && !interner.is_shared_type(ty)
        && !matches!(interner.kind(ty), TyKind::Js)
}

/// Typed unique-destroy skips the RC RMW and `free`s. Intra-procedural Unique is not object
/// uniqueness: field/index snapshots, `Result.Ok` payloads, and Map slots keep aliases. Ordinary
/// `Release` still destroys when last (`dream_rc_last`).
pub(crate) fn can_unique_destroy(_interner: &TypeInterner, _ty: TypeId) -> bool {
    false
}

fn is_fresh_alloc(rvalue: &Rvalue) -> bool {
    matches!(
        rvalue,
        Rvalue::New { .. }
            | Rvalue::UnionNew { .. }
            | Rvalue::ArrayLit { .. }
            | Rvalue::Tuple { .. }
    )
}

/// `list.get` / method results may be retained aliases of callee-owned storage.
fn call_result_may_alias(rvalue: &Rvalue) -> bool {
    matches!(
        rvalue,
        Rvalue::Call { .. }
            | Rvalue::IndirectCall { .. }
            | Rvalue::InterfaceCall { .. }
            | Rvalue::JsCall { .. }
    )
}

/// `CaptureCell` env is copied/cast to `int` before `$funcbox_new`. That alias is owned by the
/// box, so Unique last-use destroy of the cell would free under the funcbox.
fn pointer_pun_escape(rvalue: &Rvalue, dest: u32, is_owned: &dyn Fn(u32) -> bool) -> Option<u32> {
    if is_owned(dest) {
        return None;
    }
    let src = match rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(s)))
        | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _) => s.0,
        _ => return None,
    };
    if is_owned(src) {
        Some(src)
    } else {
        None
    }
}

pub(crate) fn apply_stmt_unique(
    stmt: &Statement,
    interner: &TypeInterner,
    is_owned: &dyn Fn(u32) -> bool,
    assign_is_move: bool,
    _sink_is_move: impl Fn(u32) -> bool,
    unique: &mut [bool],
) {
    let dest_local = match stmt {
        Statement::Assign(Place::Local(d), _) => Some(d.0),
        _ => None,
    };
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        if let Some(src) = pointer_pun_escape(rvalue, dest.0, is_owned) {
            unique[src as usize] = false;
        }
        if is_owned(dest.0) {
            let self_ref = rvalue_reads_local(rvalue, dest.0);
            if !self_ref {
                if is_borrowed_copy(rvalue, interner) {
                    if let Some(src) = move_source(rvalue, is_owned) {
                        if assign_is_move {
                            unique[dest.0 as usize] = unique[src.0 as usize];
                            unique[src.0 as usize] = false;
                        } else {
                            unique[dest.0 as usize] = false;
                            unique[src.0 as usize] = false;
                        }
                    } else {
                        unique[dest.0 as usize] = false;
                    }
                } else {
                    unique[dest.0 as usize] = !call_result_may_alias(rvalue);
                }
            }
        }
    }
    for src in container_move_locals(stmt) {
        if is_owned(src) {
            unique[src as usize] = false;
        }
    }
    for src in constructed_payload_locals(stmt) {
        // `list = Cons(x, list)`: payload sharing applies to the *old* value, not the new dest.
        if is_owned(src) && dest_local != Some(src) {
            unique[src as usize] = false;
        }
    }
    for local in super::tokens::call_escape_locals(stmt, is_owned) {
        if dest_local != Some(local) {
            unique[local as usize] = false;
        }
    }
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        if is_owned(dest.0) && is_fresh_alloc(rvalue) {
            unique[dest.0 as usize] = true;
        }
    }
}

pub(crate) fn meet_unique(a: bool, b: bool) -> bool {
    a && b
}
