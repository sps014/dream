//! Provenance for direct region allocations and local aliases; zero denotes a rewound origin.

use super::operands::{operand_locals, other_stmt_locals, rvalue_local_operands, terminator_reads};
use super::{violation, Violation};
use crate::{MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Origins = BTreeMap<u32, BTreeSet<usize>>;

pub(super) fn check(
    f: &MirFunction,
    interner: &TypeInterner,
    depths: &[Option<usize>],
    out: &mut Vec<Violation>,
) {
    if !f
        .blocks
        .iter()
        .any(|b| b.stmts.iter().any(|s| matches!(s, Statement::RegionEnter)))
    {
        return;
    }
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(Origins::new());
    let mut pending = VecDeque::from([f.entry]);
    while let Some(bi) = pending.pop_front() {
        let mut origins = incoming[bi.0 as usize]
            .clone()
            .expect("queued reachable block");
        let mut depth = depths[bi.0 as usize].expect("balanced reachable region stack");
        for stmt in &f.blocks[bi.0 as usize].stmts {
            transfer(stmt, f, interner, &mut depth, &mut origins);
        }
        if let Terminator::Await {
            dest: Some(dest), ..
        } = &f.blocks[bi.0 as usize].terminator
        {
            origins.remove(&dest.0);
        }
        for successor in f.blocks[bi.0 as usize].terminator.successors() {
            let row = &mut incoming[successor.0 as usize];
            let mut changed = row.is_none();
            let joined = row.get_or_insert_with(Origins::new);
            for (&local, sources) in &origins {
                let entry = joined.entry(local).or_default();
                let old_len = entry.len();
                entry.extend(sources.iter().copied());
                changed |= entry.len() != old_len;
            }
            if changed {
                pending.push_back(successor);
            }
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        let Some(mut origins) = incoming[bi].clone() else {
            continue;
        };
        let mut depth = depths[bi].expect("balanced reachable region stack");
        for (si, stmt) in block.stmts.iter().enumerate() {
            let mut reads = match stmt {
                Statement::Assign(place, rv) => {
                    let mut reads = rvalue_local_operands(rv);
                    if !matches!(place, Place::Local(_)) {
                        operand_locals(&Operand::Copy(place.clone()), &mut reads);
                    }
                    reads
                }
                Statement::Retain(op) | Statement::Release(op) => {
                    let mut reads = Vec::new();
                    operand_locals(op, &mut reads);
                    reads
                }
                _ => other_stmt_locals(stmt),
            };
            reads.sort_unstable();
            reads.dedup();
            check_reads(f, bi, si, reads, &origins, out);
            transfer(stmt, f, interner, &mut depth, &mut origins);
        }
        check_reads(
            f,
            bi,
            block.stmts.len(),
            terminator_reads(&block.terminator),
            &origins,
            out,
        );
    }
}

fn check_reads(
    f: &MirFunction,
    bi: usize,
    si: usize,
    reads: impl IntoIterator<Item = u32>,
    origins: &Origins,
    out: &mut Vec<Violation>,
) {
    for local in reads {
        if origins
            .get(&local)
            .is_some_and(|sources| sources.contains(&0))
        {
            out.push(violation(
                f,
                bi,
                si,
                format!("_{local} used after its allocation region was left"),
            ));
        }
    }
}

fn transfer(
    stmt: &Statement,
    f: &MirFunction,
    interner: &TypeInterner,
    depth: &mut usize,
    origins: &mut Origins,
) {
    match stmt {
        Statement::RegionEnter => *depth += 1,
        Statement::RegionLeave => {
            for sources in origins.values_mut() {
                if sources.remove(depth) {
                    sources.insert(0);
                }
            }
            *depth -= 1;
        }
        Statement::Assign(Place::Local(dest), rv) => {
            let sources = match rv {
                Rvalue::Use(Operand::Copy(Place::Local(src)))
                | Rvalue::Cast(Operand::Copy(Place::Local(src)), _, _)
                | Rvalue::Move { src, .. } => origins.get(&src.0).cloned(),
                Rvalue::UnionNew { ty, args, .. } if interner.is_niche_union(*ty) => {
                    match args.first() {
                        Some(Operand::Copy(Place::Local(src))) => origins.get(&src.0).cloned(),
                        _ => None,
                    }
                }
                Rvalue::UnionField {
                    base: Operand::Copy(Place::Local(src)),
                    ty,
                    ..
                } if interner.is_niche_union(*ty) => origins.get(&src.0).cloned(),
                Rvalue::New { .. }
                | Rvalue::UnionNew { .. }
                | Rvalue::ArrayLit { .. }
                | Rvalue::ArrayNew { .. }
                    if *depth > 0 && interner.is_rc_tracked(f.local_ty(*dest)) =>
                {
                    Some(BTreeSet::from([*depth]))
                }
                _ => None,
            };
            if let Some(sources) = sources {
                origins.insert(dest.0, sources);
            } else {
                origins.remove(&dest.0);
            }
        }
        _ => {}
    }
}
