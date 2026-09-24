//! Replace a substring that is only asked for its length.
//!
//! `s.substring(start, end).byte_size()` still builds a slice object: a header, a pointer at
//! the parent, and a retain. When nothing else uses that slice, the length is the clamped
//! span and the allocation is wasted. A constant span of a constant string becomes a constant.

use crate::{Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::{DefId, PrimTy, TyKind, TypeInterner};
use std::collections::{HashMap, HashSet};

pub(crate) const STAGE: &str = "slice-measure";

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let subs: HashSet<DefId> = mir
        .intrinsics
        .iter()
        .filter(|(_, key)| *key == dream_abi::intrinsics::ATTR_STRING_SUBSTRING)
        .map(|(def, _)| *def)
        .collect();
    if subs.is_empty() {
        return false;
    }
    let mut changed = false;
    for f in &mut mir.functions {
        changed |= function(f, interner, &subs);
    }
    changed
}

fn function(func: &mut MirFunction, interner: &TypeInterner, subs: &HashSet<DefId>) -> bool {
    let mut n_of: HashMap<u32, i64> = HashMap::new();
    let mut bad: HashSet<u32> = HashSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(dest), rv) = stmt else {
                continue;
            };
            if is_null(rv) {
                continue;
            }
            if let Some(n) = substring_len(func, rv, subs) {
                match n_of.get(&dest.0) {
                    Some(prev) if *prev != n => {
                        bad.insert(dest.0);
                    }
                    _ => {
                        n_of.insert(dest.0, n);
                    }
                }
            } else if !is_forward(rv) {
                bad.insert(dest.0);
            }
        }
    }
    n_of.retain(|id, _| !bad.contains(id));
    if n_of.is_empty() {
        return false;
    }
    // A copy of a measured slice is the same span. A second, different source is not.
    let mut copies: Vec<(u32, u32)> = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(dest), Rvalue::Use(Operand::Copy(Place::Local(src)))) =
                stmt
            {
                if !is_only_null_or_copy(func, dest.0) {
                    continue;
                }
                copies.push((dest.0, src.0));
            }
        }
    }
    loop {
        let mut grew = false;
        for &(dest, src) in &copies {
            if bad.contains(&dest) {
                continue;
            }
            if let Some(&n) = n_of.get(&src) {
                match n_of.get(&dest).copied() {
                    Some(prev) if prev != n => {
                        bad.insert(dest);
                    }
                    Some(_) => {}
                    None => {
                        n_of.insert(dest, n);
                        grew = true;
                    }
                }
            }
        }
        n_of.retain(|id, _| !bad.contains(id));
        if !grew {
            break;
        }
    }
    for block in &func.blocks {
        for stmt in &block.stmts {
            for id in n_of.keys().copied().collect::<Vec<_>>() {
                if !super::rc::liveness::stmt_reads_local(stmt, id) {
                    continue;
                }
                if !use_ok(stmt, id) {
                    bad.insert(id);
                }
            }
        }
    }
    n_of.retain(|id, _| !bad.contains(id));
    if n_of.is_empty() {
        return false;
    }
    let local_tys: Vec<dream_types::TypeId> = func.locals.iter().map(|l| l.ty).collect();
    let mut changed = false;
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            match stmt {
                Statement::Assign(Place::Local(dest), rv) if n_of.contains_key(&dest.0) => {
                    if matches!(rv, Rvalue::Call { .. }) {
                        *stmt = Statement::Nop;
                        changed = true;
                    }
                }
                Statement::Retain(op) | Statement::Release(op) | Statement::ReleaseUnique(op)
                    if local_op(op).is_some_and(|id| n_of.contains_key(&id)) =>
                {
                    *stmt = Statement::Nop;
                    changed = true;
                }
                Statement::Assign(place, rv) => {
                    if let Some(n) = measured(rv, &n_of) {
                        let wide = place_is_wide(&local_tys, interner, place);
                        let c = if wide { Const::Long(n) } else { Const::Int(n) };
                        *rv = Rvalue::Use(Operand::Const(c));
                        changed = true;
                    }
                }
                _ => {}
            }
        }
    }
    changed
}

fn is_null(rv: &Rvalue) -> bool {
    matches!(rv, Rvalue::Use(Operand::Const(Const::Null)))
}

fn is_forward(rv: &Rvalue) -> bool {
    matches!(rv, Rvalue::Use(Operand::Copy(Place::Local(_))))
}

fn is_only_null_or_copy(func: &MirFunction, id: u32) -> bool {
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if d.0 != id || is_null(rv) || is_forward(rv) {
                continue;
            }
            return false;
        }
    }
    true
}

fn use_ok(stmt: &Statement, id: u32) -> bool {
    match stmt {
        Statement::Retain(op) | Statement::Release(op) | Statement::ReleaseUnique(op) => {
            local_op(op) == Some(id)
        }
        Statement::Assign(Place::Local(dest), Rvalue::Call { .. }) => dest.0 == id,
        Statement::Assign(
            Place::Local(_),
            Rvalue::StrLen(op) | Rvalue::StrByteSize(op),
        ) => local_op(op) == Some(id),
        Statement::Assign(Place::Local(_), Rvalue::Use(Operand::Copy(Place::Local(src)))) => {
            src.0 == id
        }
        _ => false,
    }
}

fn measured(rv: &Rvalue, n_of: &HashMap<u32, i64>) -> Option<i64> {
    match rv {
        Rvalue::StrLen(op) => local_op(op).and_then(|id| n_of.get(&id).copied()),
        Rvalue::StrByteSize(op) => local_op(op).and_then(|id| n_of.get(&id).map(|n| n * 2)),
        _ => None,
    }
}

fn local_op(op: &Operand) -> Option<u32> {
    match op {
        Operand::Copy(Place::Local(Local(id))) => Some(*id),
        _ => None,
    }
}

fn substring_len(func: &MirFunction, rv: &Rvalue, subs: &HashSet<DefId>) -> Option<i64> {
    let Rvalue::Call { callee, args } = rv else {
        return None;
    };
    if !subs.contains(&callee.def) || callee.take_params.iter().any(|t| *t) || args.len() != 3 {
        return None;
    }
    let len = string_units(func, &args[0], &mut HashSet::new())?;
    let start = const_int(func, &args[1], &mut HashSet::new())?;
    let end = const_int(func, &args[2], &mut HashSet::new())?;
    Some(slice_n(len, start, end))
}

fn slice_n(len: i64, start: i64, end: i64) -> i64 {
    let start = clamp(start, len);
    let end = clamp(end, len);
    if end < start {
        0
    } else {
        end - start
    }
}

fn clamp(v: i64, len: i64) -> i64 {
    if v < 0 {
        0
    } else if v > len {
        len
    } else {
        v
    }
}

fn string_units(func: &MirFunction, op: &Operand, seen: &mut HashSet<u32>) -> Option<i64> {
    match op {
        Operand::Const(Const::Str(s)) => Some(s.encode_utf16().count() as i64),
        Operand::Copy(Place::Local(Local(id))) => {
            if !seen.insert(*id) {
                return None;
            }
            let mut found = None;
            for block in &func.blocks {
                for stmt in &block.stmts {
                    let Statement::Assign(Place::Local(d), rv) = stmt else {
                        continue;
                    };
                    if d.0 != *id || is_null(rv) {
                        continue;
                    }
                    if found.is_some() {
                        return None;
                    }
                    found = match rv {
                        Rvalue::Use(inner) => Some(string_units(func, inner, seen)?),
                        _ => return None,
                    };
                }
            }
            found
        }
        _ => None,
    }
}

fn const_int(func: &MirFunction, op: &Operand, seen: &mut HashSet<u32>) -> Option<i64> {
    match op {
        Operand::Const(Const::Int(n) | Const::Long(n)) => Some(*n),
        Operand::Copy(Place::Local(Local(id))) => {
            if !seen.insert(*id) {
                return None;
            }
            let mut found = None;
            for block in &func.blocks {
                for stmt in &block.stmts {
                    let Statement::Assign(Place::Local(d), rv) = stmt else {
                        continue;
                    };
                    if d.0 != *id || is_null(rv) {
                        continue;
                    }
                    if found.is_some() {
                        return None;
                    }
                    found = match rv {
                        Rvalue::Use(inner) => Some(const_int(func, inner, seen)?),
                        _ => return None,
                    };
                }
            }
            found
        }
        _ => None,
    }
}

fn place_is_wide(local_tys: &[dream_types::TypeId], interner: &TypeInterner, place: &Place) -> bool {
    let Place::Local(Local(id)) = place else {
        return false;
    };
    let Some(ty) = local_tys.get(*id as usize) else {
        return false;
    };
    matches!(
        interner.kind(*ty),
        TyKind::Prim(PrimTy::Long | PrimTy::ULong)
    )
}
