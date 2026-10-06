//! Region graph provenance follows the CFG, including aliases, calls and inline values.

use super::operands::{operand_locals, other_stmt_locals, rvalue_local_operands, terminator_reads};
use super::region_graph::{Graph, Origins};
use super::{violation, Violation};
use crate::{MirFunction, Operand, Place, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeSet, VecDeque};

pub(super) fn check(
    f: &MirFunction,
    interner: &TypeInterner,
    returns: &super::returns::Returns,
    refs: &super::ref_types::RefTypes,
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
    let analysis = Analysis {
        f,
        interner,
        returns,
        refs,
    };
    incoming[f.entry.0 as usize] = Some(Graph::new(f, refs));
    let mut pending = VecDeque::from([f.entry]);
    while let Some(id) = pending.pop_front() {
        let bi = id.0 as usize;
        let mut graph = incoming[bi].clone().expect("reachable region graph");
        let mut depth = depths[bi].expect("balanced reachable region stack");
        for (si, stmt) in f.blocks[bi].stmts.iter().enumerate() {
            analysis.transfer(stmt, &mut depth, &mut graph, (bi, si));
        }
        if let Terminator::Await {
            dest: Some(dest), ..
        } = &f.blocks[bi].terminator
        {
            graph.define(
                &Place::Local(*dest),
                &crate::Rvalue::Use(Operand::Const(crate::Const::Null)),
                BTreeSet::new(),
                false,
                bi,
                f.blocks[bi].stmts.len(),
            );
        }
        for successor in f.blocks[bi].terminator.successors() {
            let row = &mut incoming[successor.0 as usize];
            let changed = if let Some(old) = row {
                old.join(&graph)
            } else {
                *row = Some(graph.clone());
                true
            };
            if changed {
                pending.push_back(successor);
            }
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        let Some(mut graph) = incoming[bi].clone() else {
            continue;
        };
        let mut depth = depths[bi].expect("balanced reachable region stack");
        for (si, stmt) in block.stmts.iter().enumerate() {
            let reads = match stmt {
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
            check_reads(f, bi, si, reads, &graph, out);
            if matches!(stmt, Statement::RegionLeave) && graph.escapes(depth) {
                out.push(violation(
                    f,
                    bi,
                    si,
                    "region allocation escaped through a caller or global graph".into(),
                ));
            }
            if let Some((args, facts)) = super::call_effects::effects(stmt, returns)
                && depth > 0 && !effect_origins(&facts.escaped, &args, &graph, depth).is_empty() {
                    out.push(violation(
                        f,
                        bi,
                        si,
                        "region allocation may escape through a call".into(),
                    ));
                }
            analysis.transfer(stmt, &mut depth, &mut graph, (bi, si));
        }
        check_reads(
            f,
            bi,
            block.stmts.len(),
            terminator_reads(&block.terminator),
            &graph,
            out,
        );
    }
}

fn check_reads(
    f: &MirFunction,
    bi: usize,
    si: usize,
    reads: impl IntoIterator<Item = u32>,
    graph: &Graph,
    out: &mut Vec<Violation>,
) {
    for local in reads.into_iter().collect::<BTreeSet<_>>() {
        if graph.local(local).contains(&0) {
            out.push(violation(
                f,
                bi,
                si,
                format!("_{local} used after its allocation region was left"),
            ));
        }
    }
}

fn effect_origins(
    sources: &super::returns::Sources,
    args: &[Operand],
    graph: &Graph,
    depth: usize,
) -> Origins {
    let mut origins = BTreeSet::new();
    if sources.fresh && depth > 0 {
        origins.insert(depth);
    }
    for &index in &sources.params {
        if let Some(arg) = args.get(index) {
            origins.extend(graph.operand(arg));
        }
    }
    origins
}

struct Analysis<'a> {
    f: &'a MirFunction,
    interner: &'a TypeInterner,
    returns: &'a super::returns::Returns,
    refs: &'a super::ref_types::RefTypes,
}

impl Analysis<'_> {
    fn transfer(&self, stmt: &Statement, depth: &mut usize, graph: &mut Graph, at: (usize, usize)) {
        let Self {
            f,
            interner,
            returns,
            refs,
        } = self;
        if let Some((args, facts)) = super::call_effects::effects(stmt, returns) {
            let old = graph.clone();
            for (index, sources) in facts.writes {
                if let Some(arg) = args.get(index) {
                    let origins = effect_origins(&sources, &args, &old, *depth);
                    graph.write(arg, &origins);
                }
            }
        }
        match stmt {
            Statement::RegionEnter => *depth += 1,
            Statement::RegionLeave => {
                graph.rewind(*depth);
                *depth -= 1;
            }
            Statement::Assign(place, rv) => {
                let dest = match place {
                    Place::Local(l)
                    | Place::Field { base: l, .. }
                    | Place::Index { base: l, .. }
                    | Place::Deref { ptr: l, .. } => Some(*l),
                    Place::Global(_) => None,
                };
                let carries_refs = dest.is_none_or(|l| refs.contains(f.local_ty(l)));
                let sources = if carries_refs {
                    let locals: Vec<_> = (0..f.locals.len())
                        .map(|l| super::returns::Sources {
                            fresh: false,
                            params: graph.local(l as u32),
                        })
                        .collect();
                    super::returns::rvalue_sources(rv, interner, returns, &locals)
                } else {
                    super::returns::Sources::default()
                };
                let mut origins = sources.params;
                if sources.fresh && *depth > 0 {
                    origins.insert(*depth);
                }
                if matches!(place, Place::Local(_) | Place::Global(_)) {
                    if !carries_refs {
                        graph.clear(place);
                        return;
                    }
                    graph.define(place, rv, origins, sources.fresh, at.0, at.1);
                    if let crate::Rvalue::Move { src, .. } = rv
                        && !matches!(place, Place::Local(dest) if dest == src) {
                            graph.clear(&Place::Local(*src));
                        }
                    match rv {
                        crate::Rvalue::Call { callee, args } => {
                            let facts = super::returns::call_facts(callee, args.len(), returns);
                            let forwarded: Vec<_> = facts
                                .result
                                .params
                                .iter()
                                .filter_map(|&i| args.get(i).cloned())
                                .collect();
                            graph.inherit(place, &forwarded);
                        }
                        crate::Rvalue::Select {
                            then_val, else_val, ..
                        } => graph.inherit(place, &[then_val.clone(), else_val.clone()]),
                        crate::Rvalue::New { args, .. }
                        | crate::Rvalue::UnionNew { args, .. }
                        | crate::Rvalue::Tuple { elems: args, .. }
                        | crate::Rvalue::ArrayLit { elems: args, .. } => graph.inherit(place, args),
                        _ => {}
                    }
                } else if let Some(base) = dest.filter(|_| carries_refs) {
                    graph.write(&Operand::Copy(Place::Local(base)), &origins);
                }
            }
            _ => {}
        }
    }
}
