//! Scalar replacement of objects reached through several locals or holding references. Runs once
//! on the post-inline module, where layouts show the `del`, weak and unowned fields the
//! per-function [`super::Sroa`] cannot see.
//!
//! A default-constructed `o = New { ctor: None }` whose alias class ([`LocalEscape`]) is
//! [`Escape::No`] and whose members only appear as field bases, in copies between members, and in
//! RC statements becomes one local per field. A field store keeps the backend's container-store
//! rule, spelled out: a borrowed source is retained, the old occupant released, then the local
//! written; a `Move` or a fresh value is adopted without the retain ([`store_adopts`]). The
//! object's own count follows [`lifetime`]: its RC statements go, and each death releases the
//! reference-field locals.

use super::zero_for;
use crate::analysis::escape::{Escape, LocalEscape, ParamSummaries};
use crate::analysis::object_life::lifetime;
use crate::passes::licm::{stmt_reads, terminator_reads};
use crate::passes::ModulePass;
use crate::rc_store::store_adopts;
use crate::visit::{stmt_operands_mut, terminator_operands_mut};
use crate::{Const, Local, LocalDecl, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

pub struct SroaManaged;

impl ModulePass for SroaManaged {
    fn name(&self) -> &'static str {
        "sroa-managed"
    }

    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool {
        let dels: BTreeSet<String> = mir
            .functions
            .iter()
            .filter_map(|f| f.name.strip_suffix("_del").map(str::to_string))
            .collect();
        let layouts = &mir.layouts;
        let mut changed = false;
        for f in &mut mir.functions {
            if f.is_async {
                continue;
            }
            while promote_one(f, interner, layouts, &dels) {
                changed = true;
            }
        }
        changed
    }
}

fn promote_one(
    f: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    dels: &BTreeSet<String>,
) -> bool {
    let esc = LocalEscape::analyze(f, interner, &ParamSummaries::default());
    let news: Vec<(Local, TypeId)> = f
        .blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Assign(Place::Local(o), Rvalue::New { ty, ctor: None, .. }) => {
                Some((*o, *ty))
            }
            _ => None,
        })
        .collect();
    for (o, ty) in news {
        if esc.of(o) != Escape::No {
            continue;
        }
        let Some(fields) = field_types(ty, interner, layouts, dels) else {
            continue;
        };
        let members: BTreeSet<Local> = esc.class(o).into_iter().collect();
        if members.iter().any(|&m| f.local_ty(m) != ty) || !only_field_uses(f, &members) {
            continue;
        }
        let Some(life) = lifetime(f, &members.iter().copied().collect::<Vec<_>>(), o) else {
            continue;
        };
        let deaths: BTreeSet<(usize, usize)> =
            life.deaths.iter().map(|&(bi, si, _)| (bi, si)).collect();
        let rc_ops: BTreeSet<(usize, usize)> = life.rc_ops.iter().copied().collect();
        transform(f, interner, &members, &fields, &deaths, &rc_ops);
        return true;
    }
    false
}

/// Every field's type, or `None` if the object must stay whole.
fn field_types(
    ty: TypeId,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    dels: &BTreeSet<String>,
) -> Option<Vec<TypeId>> {
    if !matches!(interner.kind(ty), TyKind::Struct(..))
        || interner.is_value_type(ty)
        || interner.is_shared_type(ty)
    {
        return None;
    }
    let layout = layouts.get(ty)?;
    if dels.contains(&layout.name)
        || layout
            .fields
            .iter()
            .any(|fl| fl.is_weak || fl.is_unowned || interner.is_value_type(fl.ty))
    {
        return None;
    }
    Some(layout.fields.iter().map(|fl| fl.ty).collect())
}

fn member_field(op: &Operand, members: &BTreeSet<Local>) -> Option<(Local, usize)> {
    match op {
        Operand::Copy(Place::Field { base, field }) if members.contains(base) => {
            Some((*base, *field))
        }
        _ => None,
    }
}

fn is_member(op: &Operand, members: &BTreeSet<Local>) -> bool {
    matches!(op, Operand::Copy(Place::Local(l)) if members.contains(l))
}

/// Members appear only as field bases, in their own definitions, and in RC statements.
fn only_field_uses(f: &MirFunction, members: &BTreeSet<Local>) -> bool {
    let hidden = Local(u32::MAX);
    let mut hide = |o: &mut Operand| {
        if member_field(o, members).is_some() {
            *o = Operand::Copy(Place::Local(hidden));
        }
    };
    let mentions = |s: &Statement| {
        let mut hit = false;
        stmt_reads(s, &mut |l| hit |= members.contains(&l));
        hit
    };
    for b in &f.blocks {
        for s in &b.stmts {
            match s {
                Statement::Assign(Place::Local(d), _) if members.contains(d) => continue,
                Statement::Retain(op) | Statement::Release(op) | Statement::ReleaseUnique(op)
                    if is_member(op, members) =>
                {
                    continue
                }
                _ => {}
            }
            let mut s = s.clone();
            stmt_operands_mut(&mut s, &mut hide);
            if let Statement::Assign(Place::Field { base, .. }, rv) = &s {
                if members.contains(base) {
                    if matches!(rv, Rvalue::ArrayRealloc { .. })
                        || mentions(&Statement::Assign(Place::Local(hidden), rv.clone()))
                    {
                        return false;
                    }
                    continue;
                }
            }
            if mentions(&s) {
                return false;
            }
        }
        let mut t = b.terminator.clone();
        terminator_operands_mut(&mut t, &mut hide);
        let mut hit = false;
        terminator_reads(&t, &mut |l| hit |= members.contains(&l));
        if hit || matches!(&t, Terminator::Await { dest: Some(d), .. } if members.contains(d)) {
            return false;
        }
    }
    true
}

fn new_local(f: &mut MirFunction, ty: TypeId) -> Local {
    f.locals.push(LocalDecl {
        ty,
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
    });
    Local(f.locals.len() as u32 - 1)
}

fn transform(
    f: &mut MirFunction,
    interner: &TypeInterner,
    members: &BTreeSet<Local>,
    fields: &[TypeId],
    deaths: &BTreeSet<(usize, usize)>,
    rc_ops: &BTreeSet<(usize, usize)>,
) {
    let promo: Vec<Local> = fields.iter().map(|&ty| new_local(f, ty)).collect();
    let mut temps: BTreeMap<TypeId, Local> = BTreeMap::new();
    for &ty in fields {
        if interner.is_reference(ty) && !temps.contains_key(&ty) {
            let t = new_local(f, ty);
            temps.insert(ty, t);
        }
    }
    let is_ref: Vec<bool> = fields.iter().map(|&ty| interner.is_reference(ty)).collect();
    let mut expose = |o: &mut Operand| {
        if let Some((_, field)) = member_field(o, members) {
            *o = Operand::Copy(Place::Local(promo[field]));
        }
    };
    let copy = |l: Local| Operand::Copy(Place::Local(l));
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let old = std::mem::take(&mut block.stmts);
        for (si, mut s) in old.into_iter().enumerate() {
            if rc_ops.contains(&(bi, si)) {
                continue;
            }
            if deaths.contains(&(bi, si)) {
                for (i, &p) in promo.iter().enumerate() {
                    if is_ref[i] {
                        block.stmts.push(Statement::Release(copy(p)));
                    }
                }
                continue;
            }
            if let Statement::Assign(Place::Local(d), rv) = &s {
                if members.contains(d) {
                    if matches!(rv, Rvalue::New { .. }) {
                        for (i, &p) in promo.iter().enumerate() {
                            let zero = if is_ref[i] {
                                Const::Null
                            } else {
                                zero_for(interner, fields[i])
                            };
                            block
                                .stmts
                                .push(Statement::Assign(Place::Local(p), Rvalue::Use(Operand::Const(zero))));
                        }
                    }
                    continue;
                }
            }
            stmt_operands_mut(&mut s, &mut expose);
            let Statement::Assign(Place::Field { base, field }, rv) = s else {
                block.stmts.push(s);
                continue;
            };
            if !members.contains(&base) {
                block.stmts.push(Statement::Assign(Place::Field { base, field }, rv));
                continue;
            }
            let p = promo[field];
            if !is_ref[field] {
                block.stmts.push(Statement::Assign(Place::Local(p), rv));
                continue;
            }
            let t = temps[&fields[field]];
            let adopts = store_adopts(interner, &rv);
            let moved = match &rv {
                Rvalue::Move { src, .. } => Some(*src),
                _ => None,
            };
            let value = match rv {
                Rvalue::Move { src, cast: None } => Rvalue::Use(copy(src)),
                Rvalue::Move {
                    src,
                    cast: Some((from, to)),
                } => Rvalue::Cast(copy(src), from, to),
                other => other,
            };
            block.stmts.push(Statement::Assign(Place::Local(t), value));
            if !adopts {
                block.stmts.push(Statement::Retain(copy(t)));
            }
            block.stmts.push(Statement::Release(copy(p)));
            block
                .stmts
                .push(Statement::Assign(Place::Local(p), Rvalue::Use(copy(t))));
            if let Some(src) = moved {
                block.stmts.push(Statement::Assign(
                    Place::Local(src),
                    Rvalue::Use(Operand::Const(Const::Null)),
                ));
            }
        }
        terminator_operands_mut(&mut block.terminator, &mut expose);
    }
}
