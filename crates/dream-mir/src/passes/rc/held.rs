//! Post-inline "held by a live owner" RC removal.
//!
//! RC insertion runs before inlining and must treat every call as able to overwrite any heap
//! slot, so a snapshot loaded out of a container keeps its own count:
//!
//! ```text
//! x = base.f | base[i] | base as Some._0
//! Retain(x)
//! ... reads of x, calls ...
//! Release(x)
//! ```
//!
//! After inlining, most of those calls are gone or have a [`ModRefTable`] summary. The retain
//! is redundant when, wherever `x` is still to be read:
//!
//! - `base` is live (or a borrowed parameter) and is neither rebound, released nor given away,
//!   so its object and the strong slot `x` came from stay alive;
//! - nothing can overwrite that slot: no store to the same `(type, field)` or element type, no
//!   call / constructor / `del` whose summary may, nothing opaque, nothing reading refcounts;
//!
//! and `x` never hands its count on (moved into a container, passed as a sink, returned,
//! copied). Then every `Retain(x)` / `Release(x)` goes and `x` becomes a cursor, which makes the
//! backend retain `x` at any container store.

use super::liveness;
use super::tokens::{assigns_local, sink_call_args};
use crate::analysis::effects::ModRefTable;
use crate::analysis::ownership::OwnershipFacts;
use crate::{Const, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TypeId, TypeInterner};
use indexmap::{IndexMap, IndexSet};
use std::collections::{BTreeMap, BTreeSet};

#[path = "held_candidates.rs"]
mod candidates;
#[path = "held_check.rs"]
mod check;
use candidates::candidates;
use check::Check;

#[path = "held_proof.rs"]
mod proof;

/// `--emit-mir` stage name.
pub(crate) const STAGE: &str = "rc-held-by-owner";

/// Runs over every synchronous function; returns whether anything changed.
pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    if mir.profile.is_debug() {
        return false;
    }
    let modref = ModRefTable::compute(mir, interner);
    let layouts = mir.layouts.clone();
    let mut changed = false;
    for f in &mut mir.functions {
        changed |= run_function(f, interner, &layouts, &modref);
    }
    changed
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    /// `(base type, field)`.
    Field(TypeId, u32),
    /// An element of this array type.
    Elem(TypeId),
    /// A strong field reached through a by-reference array element.
    ElemField(TypeId, u32),
    /// An (immutable) union payload.
    Payload,
    /// The parameter itself: no heap store can replace it, only rebinding or releasing it.
    Param,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Snapshot {
    base: u32,
    slot: Slot,
    index: Option<i64>,
}

pub(crate) fn run_function(
    f: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    modref: &ModRefTable,
) -> bool {
    if f.is_async
        || f.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Await { .. }))
    {
        return false;
    }
    let mut pending = candidates(f, interner, layouts);
    let mut changed = false;
    // A snapshot of a snapshot is decided after its base, whose own RC ops may go first.
    while !pending.is_empty() {
        let ready: Vec<u32> = pending
            .iter()
            .filter(|(_, c)| !pending.contains_key(&c.snap.base))
            .map(|(&x, _)| x)
            .collect();
        if ready.is_empty() {
            break;
        }
        let mut ignored: IndexSet<u32> = pending.keys().copied().collect();
        for c in pending.values() {
            ignored.extend(c.class.iter().copied());
        }
        let live_out = liveness::live_out(&without_rc_on(f, &ignored));
        let token_live = liveness::live_out(f);
        let facts = OwnershipFacts::compute(f, interner);
        let observes = crate::analysis::ownership::observes_ownership(f, interner, layouts, modref);
        let accepted: Vec<u32> = ready
            .iter()
            .copied()
            .filter(|&x| {
                let accepted = Check {
                    observes,
                    token_live: &token_live,
                    f,
                    interner,
                    layouts,
                    modref,
                    x,
                    snap: pending[&x].snap,
                    class: &pending[&x].class,
                    ignored: &ignored,
                    facts: &facts,
                }
                .holds(&live_out);
                if !accepted {
                    tracing::debug!(pass = STAGE, function = %f.name, local = x,
                        decision = "rejected", reason = "owner-or-slot-not-covered");
                }
                accepted
            })
            .collect();
        let classes: Vec<BTreeSet<u32>> = accepted
            .iter()
            .map(|&x| pending[&x].class.clone())
            .collect();
        if crate::verify::enabled() {
            let current = candidates(f, interner, layouts);
            for &x in &accepted {
                let proof = proof::Proof::record(f, x, &pending[&x], &ignored);
                let context = Check {
                    token_live: &token_live,
                    f,
                    interner,
                    layouts,
                    modref,
                    x,
                    snap: pending[&x].snap,
                    class: &pending[&x].class,
                    ignored: &ignored,
                    facts: &facts,
                    observes,
                };
                assert!(
                    proof.validate_with(&context, &live_out, &current),
                    "invalid RC elision certificate"
                );
            }
        }
        for x in &ready {
            pending.remove(x);
        }
        for class in &classes {
            let mut removed = 0;
            for b in &mut f.blocks {
                let before = b.stmts.len();
                b.stmts
                    .retain(|s| rc_target(s).is_none_or(|l| !class.contains(&l)));
                removed += before - b.stmts.len();
            }
            for &m in class {
                f.locals[m as usize].is_cursor = true;
            }
            changed = true;
            tracing::debug!(pass = STAGE, function = %f.name, operations = removed,
                decision = "eliminated", reason = "covered-by-live-owner");
        }
    }
    changed
}

/// A retained snapshot and its unretained local aliases, whose validity rides on its count.
#[derive(Clone, Debug)]
struct Candidate {
    snap: Snapshot,
    class: BTreeSet<u32>,
}

/// `Retain(x)` follows the definition before anything else touches `x`.
fn retained_after(rest: &[Statement], x: u32) -> bool {
    for s in rest {
        if matches!(s, Statement::Retain(_)) && rc_target(s) == Some(x) {
            return true;
        }
        if super::stmt_reads_local(s, x) || assigns_local(s, x) {
            return false;
        }
    }
    false
}

/// `x` is overwritten with null before anything else reads, releases or redefines it.
fn nulled_after(rest: &[Statement], x: u32) -> bool {
    for s in rest {
        if let Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Const(Const::Null))) = s
            && d.0 == x
        {
            return true;
        }
        if super::stmt_reads_local(s, x) || assigns_local(s, x) {
            return false;
        }
    }
    false
}

fn rc_target(s: &Statement) -> Option<u32> {
    match s {
        Statement::Retain(Operand::Copy(Place::Local(l)))
        | Statement::Release(Operand::Copy(Place::Local(l))) => Some(l.0),
        _ => None,
    }
}

fn local_of(op: &Operand) -> Option<u32> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(l.0),
        _ => None,
    }
}

fn constant_index(op: &Operand) -> Option<i64> {
    match op {
        Operand::Const(Const::Int(n)) => Some(*n),
        _ => None,
    }
}

/// Locals whose count `stmt` may take over: sink arguments, container moves, aggregate
/// payloads, unconditional frees.
fn for_each_given_away(stmt: &Statement, mut out: impl FnMut(u32)) {
    if let Some((takes, args)) = sink_call_args(stmt) {
        for (i, a) in args.iter().enumerate() {
            if takes.get(i).copied().unwrap_or(false) {
                local_of(a).into_iter().for_each(&mut out);
            }
        }
    }
    match stmt {
        Statement::Assign(_, rv) => match rv {
            Rvalue::Move { src, .. } => out(src.0),
            Rvalue::UnionNew { args, .. }
            | Rvalue::ArrayLit { elems: args, .. }
            | Rvalue::Tuple { elems: args, .. }
            | Rvalue::New {
                ctor: None, args, ..
            } => args.iter().filter_map(local_of).for_each(out),
            _ => {}
        },
        Statement::ForceFree(op) => local_of(op).into_iter().for_each(out),
        _ => {}
    }
}

fn terminator_hands_on(t: &Terminator) -> Option<u32> {
    match t {
        Terminator::Return(Some(op)) | Terminator::AsyncComplete(Some(op)) => local_of(op),
        Terminator::Await { future, .. } => local_of(future),
        _ => None,
    }
    .or_else(|| match t {
        Terminator::TailCall { args, .. } => args.iter().find_map(local_of),
        _ => None,
    })
}

/// `f` without RC ops on `locals`, so their liveness is that of their real reads.
fn without_rc_on(f: &MirFunction, locals: &IndexSet<u32>) -> MirFunction {
    let mut g = MirFunction {
        borrowed_result: f.borrowed_result,
        def: f.def,
        instance: f.instance.clone(),
        name: String::new(),
        symbol: f.symbol.clone(),
        params: f.params.clone(),
        ret: f.ret,
        locals: f.locals.clone(),
        blocks: f.blocks.clone(),
        entry: f.entry,
        is_async: f.is_async,
        hir_fn: None,
        file: None,
        inline: dream_hir::InlineHint::Default,
    };
    for b in &mut g.blocks {
        b.stmts
            .retain(|s| !rc_target(s).is_some_and(|l| locals.contains(&l)));
    }
    g
}
