use super::super::lifetime::may_die_after;
use crate::Local;
use crate::MirFunction;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use dream_types::DefId;
use dream_types::TypeInterner;
use indexmap::IndexMap;
use indexmap::IndexSet;

pub(super) fn rc_snapshots_of(
    func: &MirFunction,
    interner: &TypeInterner,
) -> IndexMap<u32, Vec<u32>> {
    let mut m: IndexMap<u32, Vec<u32>> = IndexMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(dest), rv) = stmt else {
                continue;
            };
            if !interner.is_rc_tracked(func.locals[dest.0 as usize].ty) {
                continue;
            }
            let base = match rv {
                Rvalue::Use(Operand::Copy(Place::Field { base, .. }))
                | Rvalue::Use(Operand::Copy(Place::Index { base, .. }))
                | Rvalue::Cast(Operand::Copy(Place::Field { base, .. }), _, _)
                | Rvalue::Cast(Operand::Copy(Place::Index { base, .. }), _, _) => base.0,
                Rvalue::UnionField {
                    base: Operand::Copy(Place::Local(b)),
                    ..
                } => b.0,
                _ => continue,
            };
            m.entry(base).or_default().push(dest.0);
        }
    }
    m
}

/// Cursors whose borrow rides, through copies and slot loads, on each local's count.
pub(super) fn cursor_riders(func: &MirFunction) -> IndexMap<u32, Vec<u32>> {
    let mut direct: IndexMap<u32, Vec<u32>> = IndexMap::new();
    for stmt in func.blocks.iter().flat_map(|b| &b.stmts) {
        let Statement::Assign(Place::Local(d), rv) = stmt else {
            continue;
        };
        if !func.locals[d.0 as usize].is_cursor {
            continue;
        }
        let src = match rv {
            Rvalue::Use(Operand::Copy(p)) | Rvalue::Cast(Operand::Copy(p), _, _) => match p {
                Place::Local(s) => s.0,
                Place::Field { base, .. } | Place::Index { base, .. } => base.0,
                _ => continue,
            },
            Rvalue::UnionField {
                base: Operand::Copy(Place::Local(b)),
                ..
            } => b.0,
            _ => continue,
        };
        if src != d.0 {
            direct.entry(src).or_default().push(d.0);
        }
    }
    let mut out: IndexMap<u32, Vec<u32>> = IndexMap::new();
    for &root in direct.keys() {
        let mut seen: IndexSet<u32> = IndexSet::new();
        let mut stack = vec![root];
        while let Some(x) = stack.pop() {
            for &c in direct.get(&x).into_iter().flatten() {
                if seen.insert(c) {
                    stack.push(c);
                }
            }
        }
        out.insert(root, seen.into_iter().collect());
    }
    out
}

pub(super) struct DestroySite<'a> {
    pub(super) func: &'a MirFunction,
    pub(super) interner: &'a TypeInterner,
    pub(super) layouts: &'a dream_hir::LayoutTable,
    pub(super) holds: &'a IndexSet<DefId>,
    pub(super) modref: &'a super::super::modref::ModRefTable,
}

impl DestroySite<'_> {
    pub(super) fn allows(&self, stmt: &Statement, local: u32) -> bool {
        if !may_die_after(stmt, self.holds) {
            return false;
        }
        match stmt {
            Statement::Print { .. } => true,
            // Only borrowed by the callee: die when it returns, as a sink would have in its
            // frame. A `del` keeps the declared-`borrow` rule of dying at the block end.
            Statement::Call { callee, args }
            | Statement::Assign(_, Rvalue::Call { callee, args }) => {
                !self.modref.may_run_del(
                    self.func.local_ty(Local(local)),
                    self.interner,
                    self.layouts,
                ) && args.iter().enumerate().all(|(i, a)| {
                    !matches!(a, Operand::Copy(Place::Local(l)) if l.0 == local)
                        || !callee.take_params.get(i).copied().unwrap_or(false)
                })
            }
            Statement::Assign(Place::Local(dest), rv) => match rv {
                Rvalue::Use(Operand::Copy(Place::Field { base, .. })) if base.0 == local => self
                    .func
                    .locals
                    .get(dest.0 as usize)
                    .is_some_and(|d| !self.interner.is_rc_tracked(d.ty)),
                _ => false,
            },
            _ => false,
        }
    }
}
