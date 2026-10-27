//! Source-slot stability is a live-interval property, including every forwarding alias.

use super::liveness::{Liveness, stmt_reads_local};
use crate::analysis::effects::ModRefTable;
use crate::analysis::ownership::{Location, OwnershipFacts, constant_index};
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use std::collections::BTreeSet;

pub(super) struct SlotProof<'a> {
    observes: bool,
    f: &'a MirFunction,
    i: &'a TypeInterner,
    layouts: &'a LayoutTable,
    table: &'a ModRefTable,
    facts: OwnershipFacts,
    live: Liveness,
}

impl<'a> SlotProof<'a> {
    pub(super) fn new(
        f: &'a MirFunction,
        i: &'a TypeInterner,
        layouts: &'a LayoutTable,
        table: &'a ModRefTable,
    ) -> Self {
        Self {
            observes: table.release_proofs
                && crate::analysis::ownership::observes_ownership(f, i, layouts, table),
            f,
            i,
            layouts,
            table,
            facts: if table.release_proofs {
                OwnershipFacts::compute(f, i)
            } else {
                OwnershipFacts::unknown()
            },
            live: Liveness::new(f),
        }
    }

    pub(super) fn stable(&self, dest: u32, rv: &Rvalue) -> bool {
        if !self.table.release_proofs {
            return false;
        }
        if self
            .table
            .may_run_del(self.f.local_ty(Local(dest)), self.i, self.layouts)
        {
            return false;
        }
        // Before insertion the token's cleanup point is implicit. An observer after the
        // last real read could still see that token, so observations stay function-wide.
        if self.observes {
            return false;
        }
        let location = match rv {
            Rvalue::Use(Operand::Copy(Place::Field { base, field })) => Location::Field {
                base: *base,
                field: *field as u32,
            },
            Rvalue::Use(Operand::Copy(Place::Index { base, index, .. })) => Location::Element {
                base: *base,
                index: constant_index(index),
            },
            _ => return false,
        };
        let base = match location {
            Location::Field { base, .. } | Location::Element { base, .. } => base,
        };
        if self.i.is_shared_type(self.f.local_ty(base)) {
            return false;
        }
        let mut aliases = BTreeSet::from([Local(dest)]);
        loop {
            let before = aliases.len();
            for s in self.f.blocks.iter().flat_map(|b| &b.stmts) {
                if let Statement::Assign(
                    Place::Local(d),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ) = s
                    && aliases.contains(src)
                {
                    aliases.insert(*d);
                }
            }
            if before == aliases.len() {
                break;
            }
        }
        for (bi, b) in self.f.blocks.iter().enumerate() {
            if matches!(b.terminator, Terminator::Await { .. }) {
                return false;
            }
            for (si, s) in b.stmts.iter().enumerate() {
                if !aliases
                    .iter()
                    .any(|l| self.live.after_stmt(bi, si, l.0) || stmt_reads_local(s, l.0))
                {
                    continue;
                }
                if self
                    .facts
                    .invalidates(self.f, self.i, self.table, (bi, si), location)
                {
                    return false;
                }
                if matches!(s, Statement::Assign(Place::Local(d), _) if *d == base)
                    || matches!(s, Statement::Release(Operand::Copy(Place::Local(l))) if *l == base)
                    || super::cursor::stmt_sinks_local(s, base.0)
                {
                    return false;
                }
                // Before insertion an owning local can run cleanup implicitly. Until its
                // exact death is explicit, any relevant destructor remains a barrier.
                if !self.table.del().is_quiet()
                    && self
                        .f
                        .locals
                        .iter()
                        .any(|d| self.table.may_run_del(d.ty, self.i, self.layouts))
                {
                    return false;
                }
            }
        }
        true
    }
}
