//! Protect each traversed heap slot independently; parent identity never proves child identity.

use crate::analysis::effects::ModRefTable;
use crate::analysis::ownership::{Location, OwnershipFacts, constant_index};
use crate::passes::licm::stmt_reads;
use crate::passes::rc::liveness::{Liveness, stmt_reads_local};
use crate::visit::stmt_operands_mut;
use crate::{Local, MirFunction, Operand, Place, Statement};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Protection {
    stable: BTreeSet<Location>,
}

impl Protection {
    pub(super) fn compute(f: &MirFunction, i: &TypeInterner, effects: &ModRefTable) -> Self {
        let facts = OwnershipFacts::compute(f, i);
        let live = Liveness::new(f);
        let mut locations = BTreeMap::<Location, BTreeSet<Local>>::new();
        for block in &f.blocks {
            for stmt in &block.stmts {
                let Some(dest) = output(stmt) else {
                    continue;
                };
                stmt_operands_mut(&mut stmt.clone(), &mut |operand| {
                    if let Some(location) = location(operand, f, i) {
                        locations.entry(location).or_default().insert(dest);
                    }
                });
            }
        }
        locations.retain(|&location, dependent| {
            loop {
                let before = dependent.len();
                for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
                    let Some(dest) = output(stmt) else {
                        continue;
                    };
                    if !i.is_rc_tracked(f.local_ty(dest)) && !i.is_value_type(f.local_ty(dest)) {
                        continue;
                    }
                    let mut reads = false;
                    stmt_reads(stmt, &mut |l| reads |= dependent.contains(&l));
                    if reads {
                        dependent.insert(dest);
                    }
                }
                if dependent.len() == before {
                    break;
                }
            }
            f.blocks.iter().enumerate().all(|(bi, block)| {
                block.stmts.iter().enumerate().all(|(si, stmt)| {
                    !dependent
                        .iter()
                        .any(|l| live.after_stmt(bi, si, l.0) || stmt_reads_local(stmt, l.0))
                        || !facts.invalidates(f, i, effects, (bi, si), location)
                })
            })
        });
        Self {
            stable: locations.into_keys().collect(),
        }
    }

    pub(super) fn covers(&self, op: &Operand, f: &MirFunction, i: &TypeInterner) -> bool {
        location(op, f, i).is_none_or(|location| self.stable.contains(&location))
    }
}

fn output(stmt: &Statement) -> Option<Local> {
    match stmt {
        Statement::Assign(Place::Local(l), _)
        | Statement::Assign(Place::Field { base: l, .. }, _) => Some(*l),
        _ => None,
    }
}

fn location(op: &Operand, f: &MirFunction, i: &TypeInterner) -> Option<Location> {
    match op {
        Operand::Copy(Place::Field { base, field }) if i.is_reference(f.local_ty(*base)) => {
            Some(Location::Field {
                base: *base,
                field: *field as u32,
            })
        }
        Operand::Copy(Place::Index { base, index, .. }) => Some(Location::Element {
            base: *base,
            index: constant_index(index),
        }),
        _ => None,
    }
}
