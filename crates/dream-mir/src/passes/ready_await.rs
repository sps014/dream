//! Run immediately awaited async calls that can never suspend as ordinary calls.
//!
//! A poll body without an `Await` settles during its first poll, so `f = call stub(args)` followed
//! by `await f` computes exactly what a synchronous copy of that body returns. The copy skips the
//! future allocation, the frame stores and reloads, and the scheduler bookkeeping. Futures that
//! escape (stored, passed to a combinator, started, or awaited elsewhere) keep the lazy stub.
//!
//! Runs before ownership insertion, so the rewritten call is an ordinary call to RC.

mod join;

use crate::{BasicBlock, Callee, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use crate::{Terminator, async_emit};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{DefId, ModuleId, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const STAGE: &str = "ready-await";

type Key = (DefId, Vec<TypeId>);

/// Synthetic definitions for the synchronous copies; no source declaration uses this module id.
const READY_MODULE: ModuleId = ModuleId(u32::MAX);

pub(super) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let task_runtime = task_runtime_users(mir);
    let promise_all: BTreeSet<DefId> = mir
        .intrinsics
        .iter()
        .filter(|(_, key)| IntrinsicOp::from_key(key) == Some(IntrinsicOp::PromiseAll))
        .map(|(def, _)| *def)
        .collect();
    let asyncs: BTreeSet<Key> = mir
        .functions
        .iter()
        .filter(|f| f.is_async)
        .map(|f| (f.def, f.instance.clone()))
        .collect();
    let mut joins = BTreeMap::new();
    let mut ready: BTreeMap<Key, Callee> = BTreeMap::new();
    let mut copies = Vec::new();
    let mut ordinal = 0usize;
    for stub in &mir.functions {
        if !stub.is_async {
            continue;
        }
        let poll = &mir.polls[ordinal];
        ordinal += 1;
        if stub.hir_fn.is_none() || async_emit::captures_environment(poll) {
            continue;
        }
        if let Some(join) = join::recognize(poll, &promise_all, &asyncs, &task_runtime) {
            joins.insert((stub.def, stub.instance.clone()), join);
            continue;
        }
        if suspends(poll) || direct_callees(poll).any(|k| reaches(&task_runtime, &k)) {
            continue;
        }
        let copy = sync_copy(
            poll,
            DefId {
                module: READY_MODULE,
                index: ordinal as u32,
            },
        );
        ready.insert(
            (stub.def, stub.instance.clone()),
            Callee {
                borrowed_result: false,
                def: copy.def,
                args: copy.instance.clone(),
                ret: copy.ret,
                take_params: copy.locals[..copy.params.len()]
                    .iter()
                    .map(|l| l.is_take)
                    .collect(),
            },
        );
        copies.push(copy);
    }
    if ready.is_empty() {
        return false;
    }
    let mut changed = false;
    for poll in &mut mir.polls {
        changed |= join::rewrite(poll, &joins, &ready, interner);
        changed |= rewrite(poll, &ready, interner);
    }
    if changed {
        mir.functions.extend(copies);
    }
    changed
}

fn suspends(poll: &MirFunction) -> bool {
    poll.blocks
        .iter()
        .any(|b| matches!(b.terminator, Terminator::Await { .. }))
}

/// Settles in place of the frame: `AsyncComplete` returns the value to the caller instead.
fn sync_copy(poll: &MirFunction, def: DefId) -> MirFunction {
    let blocks = poll
        .blocks
        .iter()
        .map(|b| BasicBlock {
            terminator: match &b.terminator {
                Terminator::AsyncComplete(v) => Terminator::Return(v.clone()),
                t => t.clone(),
            },
            ..b.clone()
        })
        .collect();
    MirFunction {
        borrowed_result: false,
        def,
        // Not an identifier, so no source declaration can mangle to the same symbol.
        symbol: format!("{}.ready", poll.symbol),
        instance: poll.instance.clone(),
        name: poll.name.clone(),
        params: poll.params.clone(),
        ret: poll.ret,
        locals: poll.locals.clone(),
        blocks,
        entry: poll.entry,
        is_async: false,
        hir_fn: None,
        file: poll.file.clone(),
        inline: poll.inline,
    }
}

/// Functions that reach a helper acting on the running task, directly or through synchronous
/// calls. Code moved out of its own poll would act on its caller's task instead.
fn task_runtime_users(mir: &Mir) -> BTreeSet<Key> {
    let mut users: BTreeSet<Key> = mir
        .intrinsics
        .iter()
        .filter(|(_, key)| IntrinsicOp::reads_running_task(key))
        .map(|(def, _)| (*def, Vec::new()))
        .collect();
    loop {
        let before = users.len();
        for f in mir.functions.iter().filter(|f| !f.is_async) {
            let key = (f.def, f.instance.clone());
            if !users.contains(&key) && direct_callees(f).any(|k| reaches(&users, &k)) {
                users.insert(key);
            }
        }
        if users.len() == before {
            return users;
        }
    }
}

fn reaches(users: &BTreeSet<Key>, callee: &Key) -> bool {
    users.contains(callee) || users.contains(&(callee.0, Vec::new()))
}

fn direct_callees(f: &MirFunction) -> impl Iterator<Item = Key> + '_ {
    f.blocks.iter().flat_map(|b| {
        let calls = b.stmts.iter().filter_map(|s| match s {
            Statement::Call { callee, .. } | Statement::Assign(_, Rvalue::Call { callee, .. }) => {
                Some((callee.def, callee.args.clone()))
            }
            _ => None,
        });
        let tail = match &b.terminator {
            Terminator::TailCall { callee, .. } => Some((callee.def, callee.args.clone())),
            _ => None,
        };
        calls.chain(tail)
    })
}

fn rewrite(poll: &mut MirFunction, ready: &BTreeMap<Key, Callee>, interner: &TypeInterner) -> bool {
    let mentions = mentions(poll);
    let types: Vec<TypeId> = poll.locals.iter().map(|l| l.ty).collect();
    let mut changed = false;
    for block in &mut poll.blocks {
        let Terminator::Await {
            future: Operand::Copy(Place::Local(fut)),
            dest,
            resume,
        } = block.terminator
        else {
            continue;
        };
        if mentions[fut.0 as usize] != 2 {
            continue;
        }
        let Some(at) = block
            .stmts
            .iter()
            .rposition(|s| !matches!(s, Statement::SourceLine(_) | Statement::DebugLine(_)))
        else {
            continue;
        };
        let Statement::Assign(Place::Local(target), Rvalue::Call { callee, args }) =
            &block.stmts[at]
        else {
            continue;
        };
        if *target != fut {
            continue;
        }
        let Some(direct) = ready.get(&(callee.def, callee.args.clone())) else {
            continue;
        };
        let fits = match dest {
            Some(d) => types[d.0 as usize] == direct.ret,
            // A discarded owned result would strand its `+1`.
            None => !interner.is_rc_tracked(direct.ret) && !interner.is_value_type(direct.ret),
        };
        if !fits {
            continue;
        }
        let (callee, args) = (direct.clone(), args.clone());
        block.stmts[at] = match dest {
            Some(d) => Statement::Assign(Place::Local(d), Rvalue::Call { callee, args }),
            None => Statement::Call { callee, args },
        };
        block.terminator = Terminator::Goto(resume);
        changed = true;
    }
    changed
}

/// How often each local is read or written anywhere in `f`.
fn mentions(f: &MirFunction) -> Vec<u32> {
    let mut count = vec![0u32; f.locals.len()];
    let mut bump = |l: Local| {
        if let Some(c) = count.get_mut(l.0 as usize) {
            *c += 1;
        }
    };
    let place_base = |p: &Place| match p {
        Place::Local(l) | Place::Field { base: l, .. } | Place::Index { base: l, .. } => Some(*l),
        Place::Deref { ptr, .. } => Some(*ptr),
        Place::Global(_) => None,
    };
    for b in &f.blocks {
        for s in &b.stmts {
            let mut s = s.clone();
            match &s {
                Statement::Assign(p, rv) => {
                    if let Some(l) = place_base(p) {
                        bump(l);
                    }
                    if let Rvalue::Move { src, .. } = rv {
                        bump(*src);
                    }
                }
                Statement::ValueDrop(l) | Statement::ValueRetain(l) | Statement::ValueKill(l) => {
                    bump(*l)
                }
                _ => {}
            }
            crate::visit::stmt_operands_mut(&mut s, &mut |o| {
                if let Operand::Copy(p) = o
                    && let Some(l) = place_base(p)
                {
                    bump(l);
                }
            });
        }
        let mut t = b.terminator.clone();
        crate::visit::terminator_operands_mut(&mut t, &mut |o| {
            if let Operand::Copy(p) = o
                && let Some(l) = place_base(p)
            {
                bump(l);
            }
        });
        if let Terminator::Await { dest: Some(d), .. } = &t {
            bump(*d);
        }
    }
    count
}
