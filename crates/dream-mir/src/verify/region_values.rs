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
    returns: &super::returns::Returns,
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
            transfer(stmt, f, interner, returns, &mut depth, &mut origins);
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
            transfer(stmt, f, interner, returns, &mut depth, &mut origins);
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
    returns: &super::returns::Returns,
    depth: &mut usize,
    origins: &mut Origins,
) {
    let call = match stmt {
        Statement::Call { callee, args } | Statement::Assign(_, Rvalue::Call { callee, args }) => {
            Some((callee, args))
        }
        _ => None,
    };
    if let Some((callee, args)) = call {
        let facts = super::returns::call_facts(callee, args.len(), returns);
        let old = origins.clone();
        for (index, sources) in facts.writes {
            let Some(Operand::Copy(place)) = args.get(index) else {
                continue;
            };
            let local = match place {
                Place::Local(local)
                | Place::Field { base: local, .. }
                | Place::Index { base: local, .. }
                | Place::Deref { ptr: local, .. } => local,
                Place::Global(_) => continue,
            };
            if !interner.is_rc_tracked(f.local_ty(*local)) {
                continue;
            }
            let mut effect = BTreeSet::new();
            if sources.fresh && *depth > 0 {
                effect.insert(*depth);
            }
            for param in sources.params {
                if let Some(arg) = args.get(param) {
                    effect.extend(operand_origins(arg, &old));
                }
            }
            origins.entry(local.0).or_default().extend(effect);
        }
    }
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
                Rvalue::Call { callee, args } if interner.is_rc_tracked(f.local_ty(*dest)) => {
                    let summary = super::returns::call_facts(callee, args.len(), returns).result;
                    let mut sources = BTreeSet::new();
                    if summary.fresh && *depth > 0 {
                        sources.insert(*depth);
                    }
                    for index in summary.params {
                        if let Some(arg) = args.get(index) {
                            sources.extend(operand_origins(arg, origins));
                        }
                    }
                    Some(sources)
                }
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
                Rvalue::Use(op) | Rvalue::Cast(op, _, _) | Rvalue::UnionField { base: op, .. } => {
                    Some(operand_origins(op, origins))
                }
                Rvalue::Select {
                    then_val, else_val, ..
                } => {
                    let mut sources = operand_origins(then_val, origins);
                    sources.extend(operand_origins(else_val, origins));
                    Some(sources)
                }
                _ if interner.is_rc_tracked(f.local_ty(*dest)) => {
                    let mut sources = BTreeSet::new();
                    for local in rvalue_local_operands(rv) {
                        if let Some(old) = origins.get(&local) {
                            sources.extend(old);
                        }
                    }
                    if *depth > 0 {
                        sources.insert(*depth);
                    }
                    Some(sources)
                }
                _ => None,
            };
            if let Some(sources) = sources {
                origins.insert(dest.0, sources);
            } else {
                origins.remove(&dest.0);
            }
        }
        Statement::Assign(place, rv) => {
            let base = match place {
                Place::Field { base, .. }
                | Place::Index { base, .. }
                | Place::Deref { ptr: base, .. } => base,
                _ => return,
            };
            if !interner.is_rc_tracked(f.local_ty(*base)) {
                return;
            }
            let mut sources = BTreeSet::new();
            for local in rvalue_local_operands(rv) {
                if let Some(old) = origins.get(&local) {
                    sources.extend(old);
                }
            }
            if *depth > 0 && crate::rc_store::rvalue_allocates(rv) {
                sources.insert(*depth);
            }
            origins.entry(base.0).or_default().extend(sources);
        }
        _ => {}
    }
}

fn operand_origins(op: &Operand, origins: &Origins) -> BTreeSet<usize> {
    match op {
        Operand::Copy(Place::Local(local))
        | Operand::Copy(Place::Field { base: local, .. })
        | Operand::Copy(Place::Index { base: local, .. })
        | Operand::Copy(Place::Deref { ptr: local, .. }) => {
            origins.get(&local.0).cloned().unwrap_or_default()
        }
        _ => BTreeSet::new(),
    }
}
