//! MIR-level store and value-local classification the IR writers lower: which value
//! locals alias another place instead of owning a copy, which stores adopt their source's count,
//! and which locals the frame-allocation pass placed in a stack buffer.

use crate::rc_store::{boxes_into_object, rvalue_allocates};
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;

fn eq_operand(a: &Operand, b: &Operand) -> bool {
    match (a, b) {
        (Operand::Copy(p1), Operand::Copy(p2)) => eq_place(p1, p2),
        (Operand::Const(c1), Operand::Const(c2)) => c1 == c2,
        _ => false,
    }
}

fn eq_place(a: &Place, b: &Place) -> bool {
    match (a, b) {
        (Place::Local(l1), Place::Local(l2)) => l1 == l2,
        (Place::Global(g1), Place::Global(g2)) => g1 == g2,
        (
            Place::Field {
                base: b1,
                field: f1,
            },
            Place::Field {
                base: b2,
                field: f2,
            },
        ) => b1 == b2 && f1 == f2,
        (
            Place::Index {
                base: b1,
                index: i1,
                ..
            },
            Place::Index {
                base: b2,
                index: i2,
                ..
            },
        ) => b1 == b2 && eq_operand(i1, i2),
        (
            Place::Deref {
                ptr: p1,
                elem_ty: t1,
            },
            Place::Deref {
                ptr: p2,
                elem_ty: t2,
            },
        ) => p1 == p2 && t1 == t2,
        _ => false,
    }
}

/// `a = realloc(a, n)`: the store writes the resized block back over its own source, so there is
/// no old value to release.
pub(crate) fn realloc_self_store(place: &Place, rv: &Rvalue) -> bool {
    let Rvalue::ArrayRealloc { array, .. } = rv else {
        return false;
    };
    let Operand::Copy(src) = array else {
        return false;
    };
    eq_place(place, src)
}

/// The stored value is borrowed (not a fresh `+1`), so the slot must take its own reference.
pub(crate) fn borrowed_ref_store(interner: &TypeInterner, rv: &Rvalue) -> bool {
    !rvalue_allocates(rv) && !boxes_into_object(interner, rv)
}

/// The local whose reference-count token this store adopts, as recorded by `RcInsertion` or
/// the inliner (see [`crate::Rvalue::Move`]). `None` is an ordinary copy: the slot takes its
/// own reference, so the store retains.
pub(crate) fn unique_move_src(rv: &Rvalue) -> Option<u32> {
    match rv {
        Rvalue::Move { src, .. } => Some(src.0),
        _ => None,
    }
}

/// A compiler temporary that is only ever assigned place copies: it points at the source value
/// instead of owning an inline copy, so it is never dropped.
pub(crate) fn is_alias_value_local(f: &MirFunction, local: Local) -> bool {
    if f.locals[local.0 as usize].name.is_some() {
        return false;
    }
    let mut seen = false;
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let Statement::Assign(Place::Local(other), rv) = stmt else {
            continue;
        };
        if *other != local {
            continue;
        }
        seen = true;
        if !is_value_place_alias(f, local, rv) {
            return false;
        }
    }
    seen
}

pub(crate) fn is_value_copy_local(f: &MirFunction, local: Local) -> bool {
    let mut seen = false;
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let Statement::Assign(Place::Local(other), rv) = stmt else {
            continue;
        };
        if *other != local {
            continue;
        }
        seen = true;
        if !matches!(rv, Rvalue::Use(_)) {
            return false;
        }
    }
    seen
}

pub(crate) fn is_moved_into_union(f: &MirFunction, local: Local) -> bool {
    f.blocks.iter().flat_map(|block| &block.stmts).any(|stmt| {
        let Statement::Assign(_, rv) = stmt else {
            return false;
        };
        let Rvalue::UnionNew { args, .. } = rv else {
            return false;
        };
        args.iter()
            .any(|arg| matches!(arg, Operand::Copy(Place::Local(src)) if *src == local))
    })
}

fn is_place_copy(rv: &Rvalue) -> bool {
    matches!(
        rv,
        Rvalue::Use(Operand::Copy(Place::Local(_)))
            | Rvalue::Use(Operand::Copy(Place::Index { .. }))
            | Rvalue::Use(Operand::Copy(Place::Field { .. }))
            | Rvalue::Use(Operand::Copy(Place::Deref { .. }))
            | Rvalue::UnionField { .. }
    )
}

pub(crate) fn is_value_place_alias(f: &MirFunction, local: Local, rv: &Rvalue) -> bool {
    if f.locals[local.0 as usize].name.is_some() || !is_place_copy(rv) {
        return false;
    }
    f.blocks
        .iter()
        .flat_map(|block| &block.stmts)
        .all(|stmt| match stmt {
            Statement::Assign(Place::Local(other), rv) if *other == local => is_place_copy(rv),
            _ => true,
        })
}

/// `local`'s `New` is built in a stack frame buffer (see `passes::frame_alloc`).
pub(crate) fn has_frame_buffer(mir: &crate::Mir, f: &MirFunction, local: Local) -> bool {
    mir.frame_objects
        .contains(&(f.def, f.instance.clone(), local))
}

/// Value locals a sync return tears down (their reference fields released), skipping `skip`.
pub(crate) fn teardown_value_locals(
    interner: &TypeInterner,
    f: &MirFunction,
    skip: Option<Local>,
) -> Vec<Local> {
    let mut dropped = vec![false; f.locals.len()];
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        if let Statement::ValueDrop(l) = stmt
            && !f.locals[l.0 as usize].is_ref {
                dropped[l.0 as usize] = true;
            }
    }
    let mut out = Vec::new();
    for (i, decl) in f.locals.iter().enumerate() {
        let local = Local(i as u32);
        if skip == Some(local)
            || decl.manual_drop
            || decl.is_ref
            || dropped[i]
            || !interner.is_value_type(decl.ty)
            || is_alias_value_local(f, local)
            || is_value_copy_local(f, local)
            || is_moved_into_union(f, local)
        {
            continue;
        }
        if f.params.iter().any(|p| p.0 == local.0) && decl.name.as_deref() == Some("this") {
            continue;
        }
        out.push(local);
    }
    out
}
