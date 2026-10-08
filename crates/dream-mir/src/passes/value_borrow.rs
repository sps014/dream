//! Value-struct locals whose reference fields only ever hold frame-stable values borrow them
//! instead of owning a count. Frame-stable means alive for the whole call without this frame's
//! help: a `borrow` parameter (the caller keeps it alive), an interned literal, or a value loaded
//! back out of another borrowing local. A span over a borrowed string or array then costs no
//! reference counting at all.
//!
//! Release also admits a fresh private string owner whose existing token covers every view
//! and derived-reference read. The separate ownership dataflow rejects early release on any
//! path, transfers, publication and observing callbacks; it never extends the owner's cleanup.
//!
//! A family is the value locals of one type joined by whole copies. It qualifies when its members
//! are defined only by a zeroing `New`, copies between members, or are `borrow`/`ref`/`this` parameters
//! (never written); are otherwise used only as field bases or as `borrow`/`ref`/`this` arguments of callees
//! that never store a reference field of that type; and every reference field store into a member
//! is frame-stable. Members are marked [`crate::LocalDecl::borrows_refs`] (the backend stores their
//! reference fields raw) and owning ones `manual_drop`; their `ValueRetain`/`ValueDrop` glue goes.
//! Families and stable locals depend on each other, so both shrink together to a fixpoint.

use super::licm::{stmt_reads, terminator_reads};
use crate::visit::{stmt_operands_mut, terminator_operands_mut};
use crate::{
    Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_hir::LayoutTable;
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

#[path = "value_borrow_owned.rs"]
mod owned;

#[cfg(test)]
#[path = "value_borrow_tests.rs"]
mod tests;

pub(crate) const STAGE: &str = "value-borrow";

/// Per callee instance: which parameters borrow the caller's references and never store a reference
/// field of their type, so a borrowing member may be passed there.
type Signatures = BTreeMap<(DefId, Vec<TypeId>), Vec<bool>>;

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let sigs = signatures(mir, interner);
    let modref =
        (!mir.profile.is_debug()).then(|| super::rc::modref::ModRefTable::compute(mir, interner));
    let panics = panic_defs(mir);
    let mut changed = false;
    for f in &mut mir.functions {
        if !f.is_async {
            changed |= borrow_families(f, interner, &mir.layouts, &sigs, modref.as_ref(), &panics);
        }
    }
    changed
}

pub(super) fn panic_defs(mir: &Mir) -> BTreeSet<DefId> {
    mir.intrinsics
        .iter()
        .filter_map(|(def, key)| {
            (dream_abi::intrinsics::IntrinsicOp::from_key(key)
                == Some(dream_abi::intrinsics::IntrinsicOp::Panic))
            .then_some(*def)
        })
        .collect()
}

fn signatures(mir: &Mir, interner: &TypeInterner) -> Signatures {
    mir.functions
        .iter()
        .map(|f| {
            let uses = f
                .params
                .iter()
                .map(|&p| {
                    let d = &f.locals[p.0 as usize];
                    borrows_caller(f, p)
                        && interner.is_value_type(d.ty)
                        && !stores_ref_field_of(f, d.ty, interner)
                        && !passes_along(f, p)
                })
                .collect();
            ((f.def, f.instance.clone()), uses)
        })
        .collect()
}

fn borrows_caller(f: &MirFunction, p: Local) -> bool {
    let d = &f.locals[p.0 as usize];
    f.params.contains(&p) && !d.is_take
}

fn stores_ref_field_of(f: &MirFunction, ty: TypeId, interner: &TypeInterner) -> bool {
    f.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(s, Statement::Assign(Place::Field { base, .. }, rv)
            if f.local_ty(*base) == ty && !scalar_rvalue(f, interner, rv))
    })
}

fn scalar_rvalue(f: &MirFunction, interner: &TypeInterner, rv: &Rvalue) -> bool {
    match rv {
        Rvalue::Use(Operand::Copy(Place::Local(l))) => !interner.is_rc_tracked(f.local_ty(*l)),
        Rvalue::Use(Operand::Const(c)) => !matches!(c, Const::Str(_) | Const::Null),
        Rvalue::Binary(..) => true,
        _ => false,
    }
}

/// A borrowed parameter's whole-value copies must not escape or be forwarded through an
/// opaque call. Checking the original parameter alone misses escapes through named copies.
fn passes_along(f: &MirFunction, p: Local) -> bool {
    let mut aliases = BTreeSet::from([p]);
    loop {
        let mut changed = false;
        for s in f.blocks.iter().flat_map(|b| &b.stmts) {
            if let Statement::Assign(
                Place::Local(dest),
                Rvalue::Use(Operand::Copy(Place::Local(src))),
            ) = s
                && aliases.contains(src)
                && f.local_ty(*dest) == f.local_ty(p)
            {
                changed |= aliases.insert(*dest);
            }
        }
        if !changed {
            break;
        }
    }
    let hidden = Local(u32::MAX);
    for b in &f.blocks {
        for s in &b.stmts {
            match s {
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ) if aliases.contains(dest)
                    && aliases.contains(src)
                    && !f.params.contains(dest) =>
                {
                    continue;
                }
                Statement::ValueRetain(l) | Statement::ValueDrop(l) | Statement::ValueKill(l)
                    if aliases.contains(l) =>
                {
                    continue;
                }
                _ => {}
            }
            let mut s = s.clone();
            stmt_operands_mut(&mut s, &mut |o| {
                if matches!(o, Operand::Copy(Place::Field { base, .. }) if aliases.contains(base)) {
                    *o = Operand::Copy(Place::Local(hidden));
                }
            });
            let mut escapes = false;
            stmt_reads(&s, &mut |l| escapes |= aliases.contains(&l));
            if escapes {
                return true;
            }
        }
        let mut t = b.terminator.clone();
        terminator_operands_mut(&mut t, &mut |o| {
            if matches!(o, Operand::Copy(Place::Field { base, .. }) if aliases.contains(base)) {
                *o = Operand::Const(Const::Int(0));
            }
        });
        let mut escapes = false;
        terminator_reads(&t, &mut |l| escapes |= aliases.contains(&l));
        if escapes {
            return true;
        }
    }
    defines(f, p)
}

/// Every reference field of `ty` is a plain strong reference and the rest are plain scalars.
fn borrowable_type(ty: TypeId, interner: &TypeInterner, layouts: &LayoutTable) -> bool {
    if !matches!(interner.kind(ty), TyKind::Struct(..)) || !interner.is_value_type(ty) {
        return false;
    }
    let Some(layout) = layouts.get(ty) else {
        return false;
    };
    !layout.has_destructor()
        && layout.fields.iter().any(|fl| interner.is_reference(fl.ty))
        && layout.fields.iter().all(|fl| {
            !fl.is_weak
                && !fl.is_unowned
                && !interner.is_value_type(fl.ty)
                && (interner.is_reference(fl.ty) || !interner.is_rc_tracked(fl.ty))
        })
}

fn local_of(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn defines(f: &MirFunction, l: Local) -> bool {
    f.blocks.iter().any(|b| {
        b.stmts
            .iter()
            .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == l))
            || matches!(&b.terminator, Terminator::Await { dest: Some(d), .. } if *d == l)
    })
}

fn find(root: &mut [usize], mut x: usize) -> usize {
    while root[x] != x {
        root[x] = root[root[x]];
        x = root[x];
    }
    x
}

struct Ctx<'a> {
    f: &'a MirFunction,
    interner: &'a TypeInterner,
    layouts: &'a LayoutTable,
    sigs: &'a Signatures,
    /// Family root of every borrowable-typed local; `None` for every other local.
    family: Vec<Option<usize>>,
    alive: BTreeSet<usize>,
    stable: Vec<bool>,
    owned: Vec<bool>,
}

fn borrow_families(
    f: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    sigs: &Signatures,
    modref: Option<&super::rc::modref::ModRefTable>,
    panics: &BTreeSet<DefId>,
) -> bool {
    let n = f.locals.len();
    let candidate: Vec<bool> = f
        .locals
        .iter()
        .map(|d| borrowable_type(d.ty, interner, layouts))
        .collect();
    if !candidate.iter().any(|&c| c) {
        return false;
    }
    let mut root: Vec<usize> = (0..n).collect();
    for s in f.blocks.iter().flat_map(|b| &b.stmts) {
        if let Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(src)))) = s
            && candidate[d.0 as usize]
            && f.local_ty(*d) == f.local_ty(*src)
        {
            let (a, b) = (
                find(&mut root, d.0 as usize),
                find(&mut root, src.0 as usize),
            );
            root[a.max(b)] = a.min(b);
        }
    }
    let family: Vec<Option<usize>> = (0..n)
        .map(|i| candidate[i].then(|| find(&mut root, i)))
        .collect();
    let members = {
        let owners: BTreeSet<Local> = f
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .filter_map(|s| match s {
                Statement::Assign(Place::Field { base, .. }, Rvalue::Use(op))
                    if family[base.0 as usize].is_some() =>
                {
                    local_of(op)
                }
                _ => None,
            })
            .collect();
        let owned = (0..n)
            .map(|i| {
                owners.contains(&Local(i as u32))
                    && modref.is_some_and(|m| {
                        owned::source_stays_alive(
                            f,
                            Local(i as u32),
                            &family,
                            interner,
                            layouts,
                            m,
                            panics,
                        )
                    })
            })
            .collect();
        let mut cx = Ctx {
            f,
            interner,
            layouts,
            sigs,
            alive: family.iter().flatten().copied().collect(),
            family,
            owned,
            stable: (0..n)
                .map(|i| {
                    let ty = f.locals[i].ty;
                    interner.is_reference(ty) && !interner.is_value_type(ty)
                })
                .collect(),
        };
        for i in 0..n {
            if let Some(r) = cx.family[i]
                && !cx.member_shape_ok(Local(i as u32))
            {
                cx.alive.remove(&r);
            }
        }
        cx.shrink();
        (0..n)
            .filter(|&i| cx.family[i].is_some_and(|r| cx.alive.contains(&r)))
            .map(|i| Local(i as u32))
            .collect::<BTreeSet<_>>()
    };
    if members.is_empty() {
        return false;
    }
    let mut changed = false;
    for b in &mut f.blocks {
        let old_len = b.stmts.len();
        b.stmts.retain(|s| {
            !matches!(s, Statement::ValueRetain(l) | Statement::ValueDrop(l) if members.contains(l))
        });
        changed |= b.stmts.len() != old_len;
    }
    for &m in &members {
        let d = &mut f.locals[m.0 as usize];
        changed |= !d.borrows_refs || (!d.is_ref && !d.manual_drop);
        d.borrows_refs = true;
        if !d.is_ref {
            d.manual_drop = true;
        }
    }
    changed
}

impl Ctx<'_> {
    fn member(&self, l: Local) -> bool {
        self.family
            .get(l.0 as usize)
            .copied()
            .flatten()
            .is_some_and(|r| self.alive.contains(&r))
    }

    fn shrink(&mut self) {
        loop {
            let mut shrank = false;
            for i in 0..self.stable.len() {
                if self.stable[i] && !self.stable_local(Local(i as u32)) {
                    self.stable[i] = false;
                    shrank = true;
                }
            }
            let rejected: Vec<usize> = self
                .alive
                .iter()
                .copied()
                .filter(|&r| !self.family_ok(r))
                .collect();
            for r in rejected {
                self.alive.remove(&r);
                shrank = true;
            }
            if !shrank {
                return;
            }
        }
    }

    /// A member's own definitions are a zeroing `New` or a same-type copy; parameters must borrow.
    fn member_shape_ok(&self, m: Local) -> bool {
        let f = self.f;
        if f.params.contains(&m) {
            return borrows_caller(f, m) && !defines(f, m);
        }
        let ty = f.local_ty(m);
        f.blocks.iter().all(|b| {
            b.stmts.iter().all(|s| match s {
                Statement::Assign(Place::Local(l), rv) if *l == m => match rv {
                    Rvalue::New {
                        ctor: None, args, ..
                    } => args.is_empty(),
                    Rvalue::Use(Operand::Copy(Place::Local(src))) => f.local_ty(*src) == ty,
                    _ => false,
                },
                _ => true,
            }) && !matches!(&b.terminator, Terminator::Await { dest: Some(l), .. } if *l == m)
        })
    }

    /// `x` always holds a frame-stable object (or null).
    fn stable_local(&self, x: Local) -> bool {
        if self.owned[x.0 as usize] {
            return true;
        }
        let f = self.f;
        let d = &f.locals[x.0 as usize];
        if f.params.contains(&x) {
            let released = f
                .blocks
                .iter()
                .flat_map(|b| &b.stmts)
                .any(|s| matches!(s, Statement::Release(op) if local_of(op) == Some(x)));
            return !d.is_take && !d.is_ref && !released && !defines(f, x);
        }
        let mut any_def = false;
        for b in &f.blocks {
            if matches!(&b.terminator, Terminator::Await { dest: Some(l), .. } if *l == x) {
                return false;
            }
            for s in &b.stmts {
                let Statement::Assign(Place::Local(l), rv) = s else {
                    continue;
                };
                if *l != x {
                    continue;
                }
                any_def = true;
                let ok = match rv {
                    Rvalue::Use(op) => self.stable_operand(op),
                    Rvalue::Move { src, cast: None } => self.stable[src.0 as usize],
                    _ => false,
                };
                if !ok {
                    return false;
                }
            }
        }
        any_def
    }

    fn stable_operand(&self, op: &Operand) -> bool {
        match op {
            Operand::Const(c) => matches!(c, Const::Str(_) | Const::Null),
            Operand::Copy(Place::Local(l)) => self.stable[l.0 as usize],
            Operand::Copy(Place::Field { base, .. }) => self.member(*base),
            _ => false,
        }
    }

    fn field_is_ref(&self, base: Local, field: usize) -> bool {
        self.layouts
            .get(self.f.local_ty(base))
            .and_then(|l| l.fields.get(field))
            .is_some_and(|fl| self.interner.is_reference(fl.ty))
    }

    fn in_family(&self, l: Local, family: usize) -> bool {
        self.member(l) && self.family[l.0 as usize] == Some(family)
    }

    /// Whether `s` still reads a family member once member field reads are hidden.
    fn reads_whole(&self, s: &Statement, family: usize) -> bool {
        let hidden = Local(u32::MAX);
        let mut s = s.clone();
        stmt_operands_mut(&mut s, &mut |o| {
            if matches!(o, Operand::Copy(Place::Field { base, .. }) if self.in_family(*base, family))
            {
                *o = Operand::Copy(Place::Local(hidden));
            }
        });
        let mut hit = false;
        stmt_reads(&s, &mut |l| hit |= l != hidden && self.in_family(l, family));
        hit
    }

    fn family_ok(&self, family: usize) -> bool {
        let f = self.f;
        let fam = |l: Local| self.in_family(l, family);
        for b in &f.blocks {
            for s in &b.stmts {
                let ok = match s {
                    Statement::ValueRetain(l)
                    | Statement::ValueDrop(l)
                    | Statement::ValueKill(l)
                        if fam(*l) =>
                    {
                        true
                    }
                    Statement::Assign(Place::Local(d), _) if fam(*d) => true,
                    Statement::Assign(Place::Field { base, field }, rv) if fam(*base) => {
                        if f.params.contains(base) {
                            false
                        } else if self.field_is_ref(*base, *field) {
                            matches!(rv, Rvalue::Use(op) if self.stable_operand(op))
                        } else {
                            !self.reads_whole(
                                &Statement::Assign(Place::Local(*base), rv.clone()),
                                family,
                            )
                        }
                    }
                    Statement::Retain(op) | Statement::Release(op) => !matches!(
                        op,
                        Operand::Copy(Place::Local(l) | Place::Field { base: l, .. }) if fam(*l)
                    ),
                    Statement::Call { callee, args }
                    | Statement::Assign(Place::Local(_), Rvalue::Call { callee, args })
                        if args.iter().filter_map(local_of).any(fam) =>
                    {
                        self.call_borrows(callee, args, family) && {
                            let mut s = s.clone();
                            if let Statement::Call { args, .. }
                            | Statement::Assign(_, Rvalue::Call { args, .. }) = &mut s
                            {
                                args.retain(|a| !local_of(a).is_some_and(fam));
                            }
                            !self.reads_whole(&s, family)
                        }
                    }
                    _ => !self.reads_whole(s, family),
                };
                if !ok {
                    return false;
                }
            }
            let mut t = b.terminator.clone();
            terminator_operands_mut(&mut t, &mut |o| {
                if matches!(o, Operand::Copy(Place::Field { base, .. }) if fam(*base)) {
                    *o = Operand::Const(Const::Int(0));
                }
            });
            let mut hit = false;
            terminator_reads(&t, &mut |l| hit |= fam(l));
            if hit {
                return false;
            }
        }
        true
    }

    fn call_borrows(&self, callee: &Callee, args: &[Operand], family: usize) -> bool {
        let Some(uses) = self.sigs.get(&(callee.def, callee.args.clone())) else {
            return false;
        };
        args.iter().enumerate().all(|(i, a)| {
            !local_of(a).is_some_and(|l| self.in_family(l, family))
                || uses.get(i).copied().unwrap_or(false)
        })
    }
}
