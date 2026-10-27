//! A caller-held parameter token can cover a surplus pair across summarized operations.

use super::super::RcKey;
use super::postdom::{elide_postdom_by, region_all};
use crate::analysis::effects::ModRefTable;
use crate::analysis::ownership::{OwnershipFacts, statement_observes_ownership};
use crate::passes::FunctionAnalyses;
use crate::{MirFunction, Operand, Place, Statement};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use std::collections::BTreeSet;

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;

pub(super) fn run(
    f: &mut MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    effects: &ModRefTable,
) -> bool {
    let pinned: BTreeSet<_> = f
        .params
        .iter()
        .copied()
        .filter(|p| {
            let d = &f.locals[p.0 as usize];
            !d.is_take
                && !d.is_ref
                && i.is_reference(d.ty)
                && !i.is_value_type(d.ty)
                && !i.is_shared_type(d.ty)
                && !f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.stmts)
                    .any(|s| matches!(s, Statement::Assign(Place::Local(dest), _) if dest == p))
        })
        .collect();
    if pinned.is_empty() || !f.blocks.iter().flat_map(|b| &b.stmts).any(|s|
        matches!(s, Statement::Retain(Operand::Copy(Place::Local(l))) if pinned.contains(l))) {
        return false;
    }
    let mut analyses = FunctionAnalyses::default();
    let facts = OwnershipFacts::compute(f, i);
    elide_postdom_by(f, &mut analyses, |f, key, dom, pdom, rb, rs, eb, es| {
        let RcKey::Local(owner) = key else {
            return false;
        };
        if !pinned.contains(&owner) {
            return false;
        }
        region_all(f, dom, pdom, (rb, rs, eb, es), |bi, si, stmt| {
            let aliases = |op: &crate::Operand| match op {
                crate::Operand::Copy(Place::Local(l)) => facts.may_alias(bi, si, owner, *l),
                crate::Operand::Const(_) => false,
                _ => true,
            };
            if matches!(
                stmt,
                Statement::Retain(_)
                    | Statement::Release(_)
                    | Statement::ForceFree(_)
                    | Statement::RegionLeave
                    | Statement::DeferLeave(_)
                    | Statement::ValueDrop(_)
                    | Statement::ValueKill(_)
                    | Statement::ValueRetain(_)
            ) || statement_observes_ownership(f, i, layouts, effects, stmt)
            {
                return false;
            }
            if let Some((takes, arguments)) = super::super::tokens::sink_call_args(stmt)
                && arguments
                    .iter()
                    .enumerate()
                    .any(|(index, op)| takes.get(index).copied().unwrap_or(false) && aliases(op))
            {
                return false;
            }
            match stmt {
                Statement::Assign(_, crate::Rvalue::ArrayRealloc { array, .. }) => !aliases(array),
                Statement::Assign(_, crate::Rvalue::Move { src, .. }) => {
                    !facts.may_alias(bi, si, owner, *src)
                }
                Statement::Assign(
                    _,
                    crate::Rvalue::ArrayLit { elems: args, .. }
                    | crate::Rvalue::Tuple { elems: args, .. }
                    | crate::Rvalue::UnionNew { args, .. },
                ) => !args.iter().any(aliases),
                _ => true,
            }
        })
    })
}
