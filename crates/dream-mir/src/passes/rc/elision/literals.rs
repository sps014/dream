//! Immortal references stay immortal through copies and private inline value storage.

use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Slot = (Local, Vec<usize>);

pub(super) fn elide_literal_rc(
    func: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> bool {
    let literal = literal_slots(func, interner, layouts);
    let mut changed = false;
    let mut values = BTreeSet::new();
    let frame = literal
        .iter()
        .any(|(_, path)| !path.is_empty())
        .then(|| crate::backend::shared::ValueFrame::compute(func, interner));
    let mut known_fields = vec![0; func.locals.len()];
    for (local, path) in &literal {
        if !path.is_empty() {
            known_fields[local.0 as usize] += 1;
        }
    }
    for (i, decl) in func.locals.iter_mut().enumerate() {
        if known_fields[i] == 0 || !plain_value(decl.ty, interner, layouts) {
            continue;
        }
        let local = Local(i as u32);
        let mut count = 0;
        leaves(decl.ty, &mut Vec::new(), interner, layouts, &mut |_| {
            count += 1
        });
        if known_fields[i] != count {
            continue;
        }
        values.insert(local);
        let owning = frame.as_ref().and_then(|f| f.kind(local))
            == Some(crate::backend::shared::ValueLocalKind::Owning);
        changed |= !decl.borrows_refs || (owning && !decl.manual_drop);
        decl.borrows_refs = true;
        if owning {
            decl.manual_drop = true;
        }
    }
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            if matches!(stmt, Statement::ValueRetain(l) | Statement::ValueDrop(l) if values.contains(l))
            {
                *stmt = Statement::Nop;
                changed = true;
                continue;
            }
            let (Statement::Retain(op) | Statement::Release(op)) = stmt else {
                continue;
            };
            if matches!(op, Operand::Const(Const::Str(_) | Const::Null))
                || matches!(op, Operand::Copy(p) if slot(p).is_some_and(|s| literal.contains(&s)))
            {
                *stmt = Statement::Nop;
                changed = true;
            }
        }
    }
    changed
}

fn slot(place: &Place) -> Option<Slot> {
    match place {
        Place::Local(l) => Some((*l, vec![])),
        Place::Field { base, field } => Some((*base, vec![*field])),
        _ => None,
    }
}

struct Facts {
    slots: Vec<Slot>,
    index: BTreeMap<Slot, usize>,
    locals: Vec<Vec<usize>>,
    aliases: crate::union_find::UnionFind,
    defined: Vec<bool>,
    seed: Vec<bool>,
    bad: Vec<bool>,
    dependencies: Vec<(usize, usize)>,
}

impl Facts {
    fn new(f: &MirFunction, types: &TypeInterner, layouts: &LayoutTable) -> Self {
        let mut slots = Vec::new();
        let mut locals = vec![Vec::new(); f.locals.len()];
        for (i, decl) in f.locals.iter().enumerate() {
            leaves(decl.ty, &mut Vec::new(), types, layouts, &mut |path| {
                locals[i].push(slots.len());
                slots.push((Local(i as u32), path.to_vec()));
            });
        }
        let n = slots.len();
        Self {
            index: slots
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, s)| (s, i))
                .collect(),
            slots,
            locals,
            aliases: crate::union_find::UnionFind::new(n),
            defined: vec![false; n],
            seed: vec![false; n],
            bad: vec![false; n],
            dependencies: Vec::new(),
        }
    }

    fn within(&self, prefix: &Slot) -> Vec<usize> {
        self.locals
            .get(prefix.0.0 as usize)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&i| self.slots[i].1.starts_with(&prefix.1))
            .collect()
    }

    fn value_storage(&self, local: Local) -> bool {
        self.locals
            .get(local.0 as usize)
            .is_some_and(|slots| slots.iter().any(|&i| !self.slots[i].1.is_empty()))
    }

    fn poison(&mut self, prefix: &Slot) {
        for i in self.within(prefix) {
            self.bad[i] = true;
        }
    }

    fn operand(&mut self, dest: usize, op: &Operand) {
        match op {
            Operand::Const(Const::Str(_) | Const::Null) => self.seed[dest] = true,
            Operand::Copy(place) => {
                if let Some(src) = slot(place).and_then(|s| self.index.get(&s).copied()) {
                    self.dependencies.push((src, dest));
                } else {
                    self.bad[dest] = true;
                }
            }
            _ => self.bad[dest] = true,
        }
    }

    fn assign(&mut self, dest: &Slot, rv: &Rvalue) -> bool {
        let targets = self.within(dest);
        if targets.is_empty() {
            return false;
        }
        let aggregate = !self.index.contains_key(dest);
        let source = match rv {
            Rvalue::Use(Operand::Copy(p)) => slot(p),
            Rvalue::Move { src, cast: None } => Some((*src, vec![])),
            _ => None,
        };
        let zero = matches!(
            rv,
            Rvalue::Zero(_) | Rvalue::Use(Operand::Const(Const::Null))
        ) || matches!(rv, Rvalue::New { ctor: None, args, .. } if args.is_empty());
        for i in targets {
            self.defined[i] = true;
            if aggregate {
                if zero {
                    self.seed[i] = true;
                } else if let Some(src) = &source {
                    let mut path = src.1.clone();
                    path.extend_from_slice(&self.slots[i].1[dest.1.len()..]);
                    if let Some(&j) = self.index.get(&(src.0, path)) {
                        // Temporary value copies can alias their source storage. Merging even
                        // private copies is conservative when either copy is later overwritten.
                        self.aliases.union(i, j);
                    } else {
                        self.bad[i] = true;
                    }
                } else {
                    self.bad[i] = true;
                }
            } else {
                match rv {
                    Rvalue::Use(op) => self.operand(i, op),
                    Rvalue::Move { src, cast: None } => {
                        self.operand(i, &Operand::Copy(Place::Local(*src)));
                    }
                    Rvalue::Select {
                        then_val, else_val, ..
                    } => {
                        self.operand(i, then_val);
                        self.operand(i, else_val);
                    }
                    _ => self.bad[i] = true,
                }
            }
        }
        aggregate && (zero || source.is_some())
    }

    fn escape(&mut self, op: &Operand) {
        if let Operand::Copy(p) = op
            && let Some(s) = slot(p)
            && !self.index.contains_key(&s)
        {
            self.poison(&s);
        }
    }

    fn finish(mut self) -> BTreeSet<Slot> {
        if !self.seed.iter().any(|&seed| seed) {
            return BTreeSet::new();
        }
        let n = self.slots.len();
        let roots: Vec<_> = (0..n).map(|i| self.aliases.find(i)).collect();
        let mut defined = vec![false; n];
        let mut seed = vec![false; n];
        let mut bad = vec![false; n];
        let mut users = vec![Vec::new(); n];
        for (i, &root) in roots.iter().enumerate() {
            defined[root] |= self.defined[i];
            seed[root] |= self.seed[i];
            bad[root] |= self.bad[i];
        }
        for (src, dest) in self.dependencies {
            users[roots[src]].push(roots[dest]);
        }
        for i in 0..n {
            bad[i] |= !defined[i];
        }
        propagate(&mut bad, &users);
        propagate(&mut seed, &users);
        self.slots
            .into_iter()
            .enumerate()
            .filter_map(|(i, s)| (seed[roots[i]] && !bad[roots[i]]).then_some(s))
            .collect()
    }
}

fn propagate(facts: &mut [bool], users: &[Vec<usize>]) {
    let mut queue: VecDeque<_> = facts
        .iter()
        .enumerate()
        .filter_map(|(i, &yes)| yes.then_some(i))
        .collect();
    while let Some(i) = queue.pop_front() {
        for &user in &users[i] {
            if !facts[user] {
                facts[user] = true;
                queue.push_back(user);
            }
        }
    }
}

fn leaves(
    ty: TypeId,
    path: &mut Vec<usize>,
    types: &TypeInterner,
    layouts: &LayoutTable,
    emit: &mut impl FnMut(&[usize]),
) {
    if types.is_reference(ty) && !types.is_value_type(ty) {
        emit(path);
    } else if plain_value(ty, types, layouts)
        && let Some(layout) = layouts.get(ty)
    {
        for (field, info) in layout.fields.iter().enumerate() {
            if !info.is_weak && !info.is_unowned {
                path.push(field);
                leaves(info.ty, path, types, layouts, emit);
                path.pop();
            }
        }
    }
}

fn plain_value(ty: TypeId, types: &TypeInterner, layouts: &LayoutTable) -> bool {
    types.is_value_type(ty)
        && matches!(types.kind(ty), TyKind::Struct(..))
        && layouts.get(ty).is_some_and(|layout| {
            !layout.has_destructor()
                && layout.fields.iter().all(|f| {
                    !f.is_weak
                        && !f.is_unowned
                        && (!types.is_value_type(f.ty) || plain_value(f.ty, types, layouts))
                })
        })
}

fn literal_slots(f: &MirFunction, types: &TypeInterner, layouts: &LayoutTable) -> BTreeSet<Slot> {
    let mut facts = Facts::new(f, types, layouts);
    for &p in &f.params {
        facts.poison(&(p, vec![]));
    }
    for block in &f.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(place, rv) = stmt
                && let Some(dest) = slot(place)
                && facts.assign(&dest, rv)
            {
                continue;
            }
            let mut reads_storage = false;
            crate::passes::licm::stmt_reads(stmt, &mut |l| reads_storage |= facts.value_storage(l));
            if !reads_storage {
                continue;
            }
            let mut stmt = stmt.clone();
            crate::visit::stmt_operands_mut(&mut stmt, &mut |op| facts.escape(op));
            if let Statement::Assign(_, Rvalue::Move { src, .. }) = stmt {
                facts.escape(&Operand::Copy(Place::Local(src)));
            }
        }
        if let Terminator::Await { dest: Some(d), .. } = block.terminator {
            facts.poison(&(d, vec![]));
        }
        let mut reads_storage = false;
        crate::passes::licm::terminator_reads(&block.terminator, &mut |l| {
            reads_storage |= facts.value_storage(l)
        });
        if !reads_storage {
            continue;
        }
        let mut terminator = block.terminator.clone();
        crate::visit::terminator_operands_mut(&mut terminator, &mut |op| facts.escape(op));
    }
    facts.finish()
}

#[cfg(test)]
#[path = "literals_tests.rs"]
mod tests;
