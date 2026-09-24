//! Loop-carried cursor families.
//!
//! [`super::cursor::infer_cursors`] only accepts single-definition aliases, so a traversal
//! variable such as `curr = head; while … { node = curr as Some; curr = node.next }` owns a
//! reference and pays a retain/release per hop. A *family* is a connected set of RC locals whose
//! every definition is a copy, strong field load, or union payload load of another member or of
//! a *root* (an owned local or a borrow parameter). Every member then points into the graph the
//! roots hold, and stays valid while:
//!
//! - every root of a live member is itself live (so no last-use move or release of the root
//!   happens while the walk is in flight), and no root is rebound while a member is in flight;
//! - nothing in flight may overwrite a traversed `(type, field)` slot: no direct store, no
//!   callee or constructor whose [`ModRefTable`] summary includes it, no opaque call, and no
//!   `del` anywhere in the module that may store it (any release may run a `del`);
//! - no member escapes (stored, returned, moved, passed to a sink parameter, captured).
//!
//! Decrements from unrelated releases cannot free a member: each object on the path is held
//! by the strong slot that was loaded to reach it, and those slots are not overwritten.

use super::cursor::{is_null_init, mark_stmt_escapes, mark_term_escapes};
use super::liveness::{self, add_terminator_reads, transfer_stmt};
use super::modref::{stmt_effects, Effect, ModRef, ModRefTable};
use crate::{MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Clone, Copy)]
enum Src {
    Copy(u32),
    Field(u32, u32),
    Union { base: u32, ty: TypeId, variant: usize, field: usize },
}

impl Src {
    fn base(self) -> u32 {
        match self {
            Src::Copy(b) | Src::Field(b, _) | Src::Union { base: b, .. } => b,
        }
    }
}

fn family_source(func: &MirFunction, interner: &TypeInterner, dest: u32, rv: &Rvalue) -> Option<Src> {
    let rc = |ty: TypeId| interner.is_rc_tracked(ty);
    match rv {
        // A niche `Some(x)` / upcast is the same pointer under another type.
        Rvalue::Use(Operand::Copy(Place::Local(s)))
        | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _)
            if interner.is_reference(func.locals[s.0 as usize].ty)
                && interner.is_reference(func.locals[dest as usize].ty) =>
        {
            Some(Src::Copy(s.0))
        }
        Rvalue::Use(Operand::Copy(Place::Field { base, field })) => {
            Some(Src::Field(base.0, *field as u32))
        }
        Rvalue::Cast(Operand::Copy(Place::Field { base, field }), from, to)
            if rc(*from) && rc(*to) =>
        {
            Some(Src::Field(base.0, *field as u32))
        }
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(b)),
            ty,
            variant,
            field,
        } => Some(Src::Union {
            base: b.0,
            ty: *ty,
            variant: *variant,
            field: *field,
        }),
        _ => None,
    }
}

pub(crate) fn infer_cursor_families(
    func: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    modref: &ModRefTable,
) {
    if func.is_async
        || func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Await { .. }))
    {
        return;
    }
    let n = func.locals.len();
    let params: HashSet<u32> = func.params.iter().map(|p| p.0).collect();
    let mut srcs: Vec<Vec<Src>> = vec![Vec::new(); n];
    let mut shaped: Vec<bool> = vec![true; n];
    let mut defs: Vec<u32> = vec![0; n];
    let mut index_defined: Vec<bool> = vec![false; n];
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            let d = d.0 as usize;
            if is_null_init(rv) {
                continue;
            }
            defs[d] += 1;
            if matches!(
                rv,
                Rvalue::Use(Operand::Copy(Place::Index { .. }))
                    | Rvalue::Cast(Operand::Copy(Place::Index { .. }), _, _)
            ) {
                index_defined[d] = true;
            }
            match family_source(func, interner, d as u32, rv) {
                Some(s) => srcs[d].push(s),
                None => shaped[d] = false,
            }
        }
    }
    let eligible = |i: usize| {
        !params.contains(&(i as u32))
            && interner.is_rc_tracked(func.locals[i].ty)
            && !interner.is_shared_type(func.locals[i].ty)
            && shaped[i]
            && defs[i] > 0
    };
    let weak_load = |i: usize| {
        srcs[i].iter().any(|s| match *s {
            Src::Field(base, field) => layouts
                .get(func.locals[base as usize].ty)
                .and_then(|l| l.fields.get(field as usize))
                .is_some_and(|f| f.is_weak || f.is_unowned),
            _ => false,
        })
    };
    let fresh: Vec<bool> = (0..n)
        .map(|i| eligible(i) && !func.locals[i].is_cursor)
        .collect();
    // Existing cursors fed by a would-be member were validated against that member *owning*
    // its object; they join the family so they are re-validated under the family invariant.
    // Weak/unowned loads never relied on the base owning anything.
    let mut member: Vec<bool> = fresh.clone();
    let mut grew = true;
    while grew {
        grew = false;
        for i in 0..n {
            if member[i] || !eligible(i) || !func.locals[i].is_cursor || weak_load(i) {
                continue;
            }
            if srcs[i].iter().any(|s| member[s.base() as usize]) {
                member[i] = true;
                grew = true;
            }
        }
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    for i in 0..n {
        if !member[i] {
            continue;
        }
        for s in &srcs[i] {
            let b = s.base() as usize;
            if member[b] {
                let (ra, rb) = (find(&mut parent, i), find(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
    }
    let mut families: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for (i, _) in member.iter().enumerate().filter(|(_, m)| **m) {
        let r = find(&mut parent, i);
        families.entry(r).or_default().push(i as u32);
    }
    // Single-definition aliases are `infer_cursors`' job; only loop-carried shapes qualify.
    families.retain(|_, ms| ms.iter().any(|&m| fresh[m as usize] && defs[m as usize] >= 2));
    if families.is_empty() {
        return;
    }
    let member_list: Vec<u32> = families.values().flatten().copied().collect();

    // Any other strong cursor reading through a member would lose its owner; its family is off.
    let mut dependents: Vec<(u32, u32)> = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            let di = d.0 as usize;
            if !func.locals[di].is_cursor
                || member_list.contains(&d.0)
                || weak_load(di)
                || is_null_init(rv)
            {
                continue;
            }
            for &b in &member_list {
                if super::rvalue_reads_local(rv, b) {
                    dependents.push((d.0, b));
                }
            }
        }
    }

    let mut escaped: HashSet<u32> = HashSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            mark_stmt_escapes(stmt, &mut escaped);
            if let Statement::Assign(_, Rvalue::Move { src, .. }) = stmt {
                escaped.insert(src.0);
            }
        }
        mark_term_escapes(&block.terminator, &mut escaped);
    }
    let live_out = liveness::live_out(func);

    for members in families.values() {
        let ctx = FamilyCx {
            func,
            interner,
            layouts,
            modref,
            srcs: &srcs,
            index_defined: &index_defined,
            dependents: &dependents,
            member_set: members.iter().copied().collect(),
        };
        if ctx.validate(&escaped, &live_out) {
            for &m in members {
                func.locals[m as usize].is_cursor = true;
            }
        }
    }
}

type FieldKeys = BTreeSet<(TypeId, u32)>;

struct FamilyCx<'a> {
    func: &'a MirFunction,
    interner: &'a TypeInterner,
    layouts: &'a LayoutTable,
    modref: &'a ModRefTable,
    srcs: &'a [Vec<Src>],
    index_defined: &'a [bool],
    dependents: &'a [(u32, u32)],
    member_set: BTreeSet<u32>,
}

impl FamilyCx<'_> {
    fn validate(&self, escaped: &HashSet<u32>, live_out: &[HashSet<u32>]) -> bool {
        if self.member_set.iter().any(|m| escaped.contains(m)) {
            return false;
        }
        if self
            .dependents
            .iter()
            .any(|(_, base)| self.member_set.contains(base))
        {
            return false;
        }
        let roots_of = self.roots_of();
        let roots: BTreeSet<u32> = roots_of.values().flatten().copied().collect();
        if roots.is_empty() || !roots.iter().all(|&r| self.root_ok(r)) {
            return false;
        }
        let Some((fields, unions)) = self.traversed() else {
            return false;
        };
        if self.modref.del().hits(&fields, &unions) {
            return false;
        }
        // The caller holds a borrowed parameter for the whole call, dead or not.
        let borrowed: HashSet<u32> = self
            .func
            .params
            .iter()
            .filter(|p| !self.func.locals[p.0 as usize].is_take)
            .map(|p| p.0)
            .collect();
        let points_ok = |live: &HashSet<u32>| {
            self.member_set
                .iter()
                .filter(|m| live.contains(m))
                .all(|m| {
                    roots_of[m]
                        .iter()
                        .all(|r| live.contains(r) || borrowed.contains(r))
                })
        };
        let in_flight = |live: &HashSet<u32>| self.member_set.iter().any(|m| live.contains(m));
        for (bi, block) in self.func.blocks.iter().enumerate() {
            let mut live = live_out[bi].clone();
            if !points_ok(&live) {
                return false;
            }
            let after_term = in_flight(&live);
            add_terminator_reads(&block.terminator, &mut live);
            if (after_term || in_flight(&live))
                && !self.terminator_ok(&block.terminator, &fields, &unions)
            {
                return false;
            }
            if !points_ok(&live) {
                return false;
            }
            for stmt in block.stmts.iter().rev() {
                let after = in_flight(&live);
                transfer_stmt(stmt, &mut live);
                if !points_ok(&live) {
                    return false;
                }
                if (after || in_flight(&live)) && !self.stmt_ok(stmt, &roots, &fields, &unions) {
                    return false;
                }
            }
        }
        true
    }

    /// Roots each member may point into.
    fn roots_of(&self) -> BTreeMap<u32, BTreeSet<u32>> {
        let mut out: BTreeMap<u32, BTreeSet<u32>> =
            self.member_set.iter().map(|&m| (m, BTreeSet::new())).collect();
        let mut changed = true;
        while changed {
            changed = false;
            for &m in &self.member_set {
                let mut acc = out[&m].clone();
                for s in &self.srcs[m as usize] {
                    let b = s.base();
                    if self.member_set.contains(&b) {
                        acc.extend(out[&b].iter().copied());
                    } else {
                        acc.insert(b);
                    }
                }
                if acc.len() != out[&m].len() {
                    out.insert(m, acc);
                    changed = true;
                }
            }
        }
        out
    }

    fn root_ok(&self, r: u32) -> bool {
        let d = &self.func.locals[r as usize];
        if !self.interner.is_rc_tracked(d.ty) || self.interner.is_shared_type(d.ty) || d.is_cursor {
            return false;
        }
        // Mirrors `infer_cursors`: snapshots through container occupants must own. Parameters
        // qualify either way: a sink owns its +1, and a borrow is held by the caller.
        !self.index_defined[r as usize]
    }

    /// Traversed strong slots: `(base type, field)` for field loads, whole union types for
    /// payload loads. `None` if a load goes through a weak/unowned or unknown slot.
    fn traversed(&self) -> Option<(FieldKeys, BTreeSet<TypeId>)> {
        let mut fields = BTreeSet::new();
        let mut unions = BTreeSet::new();
        for &m in &self.member_set {
            for s in &self.srcs[m as usize] {
                match *s {
                    Src::Copy(_) => {}
                    Src::Field(base, field) => {
                        let ty = self.func.locals[base as usize].ty;
                        if self.interner.is_shared_type(ty) {
                            return None;
                        }
                        let f = self.layouts.get(ty)?.fields.get(field as usize)?;
                        if f.is_weak || f.is_unowned {
                            return None;
                        }
                        fields.insert((ty, field));
                    }
                    Src::Union {
                        ty, variant, field, ..
                    } => {
                        if let Some(f) = self
                            .layouts
                            .union(ty)
                            .and_then(|u| u.variants.get(variant))
                            .and_then(|v| v.fields.get(field))
                        {
                            if f.is_weak || f.is_unowned {
                                return None;
                            }
                        }
                        unions.insert(ty);
                    }
                }
            }
        }
        Some((fields, unions))
    }

    fn stores_hit(
        &self,
        s: &ModRef,
        fields: &BTreeSet<(TypeId, u32)>,
        unions: &BTreeSet<TypeId>,
    ) -> bool {
        s.hits(fields, unions)
    }

    fn stmt_ok(
        &self,
        stmt: &Statement,
        roots: &BTreeSet<u32>,
        fields: &BTreeSet<(TypeId, u32)>,
        unions: &BTreeSet<TypeId>,
    ) -> bool {
        match stmt {
            Statement::Assign(Place::Local(l), _) if roots.contains(&l.0) => return false,
            Statement::Release(Operand::Copy(Place::Local(l)))
            | Statement::ReleaseUnique(Operand::Copy(Place::Local(l)))
                if roots.contains(&l.0) =>
            {
                return false
            }
            Statement::RegionLeave => return false,
            _ => {}
        }
        let mut ok = true;
        stmt_effects(stmt, self.func, self.interner, |e| {
            let hit = match e {
                Effect::None | Effect::SlotStore(_) | Effect::GlobalStore(_) => false,
                Effect::Top => true,
                Effect::Store(base, field) => {
                    let ty = self.func.locals[base.0 as usize].ty;
                    fields.contains(&(ty, field)) || unions.contains(&ty)
                }
                Effect::Call(callee) => self.stores_hit(&self.modref.call(callee), fields, unions),
                Effect::Ctor(def) => self.stores_hit(&self.modref.ctor(def), fields, unions),
            };
            ok &= !hit;
        });
        ok
    }

    fn terminator_ok(
        &self,
        term: &Terminator,
        fields: &BTreeSet<(TypeId, u32)>,
        unions: &BTreeSet<TypeId>,
    ) -> bool {
        match term {
            Terminator::TailCall { callee, .. } => {
                !self.stores_hit(&self.modref.call(callee), fields, unions)
            }
            Terminator::Await { .. } => false,
            _ => true,
        }
    }
}
