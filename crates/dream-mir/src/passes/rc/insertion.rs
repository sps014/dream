//! [`RcInsertion`]: make reference ownership explicit in MIR.

use super::liveness::{self, live_after_stmt};
use super::{is_borrowed_copy, rvalue_reads_local};
use crate::passes::cfg;
use crate::passes::MirPass;
use crate::{
    BlockId, Const, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_types::TypeInterner;
use std::collections::{BTreeSet, HashSet};

pub struct RcInsertion;

impl MirPass for RcInsertion {
    fn name(&self) -> &'static str {
        "rc-insertion"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let local_is_ref: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.is_rc_tracked(d.ty))
            .collect();
        let params: HashSet<u32> = func.params.iter().map(|p| p.0).collect();
        let take_params: HashSet<u32> = func
            .params
            .iter()
            .copied()
            .filter(|p| func.locals[p.0 as usize].is_take)
            .map(|p| p.0)
            .collect();
        // Take params own their +1 (like ordinary owned locals). Ordinary params are borrowed.
        let is_owned_ref = |l: u32| {
            local_is_ref.get(l as usize).copied().unwrap_or(false)
                && (!params.contains(&l) || take_params.contains(&l))
        };
        let mut changed = false;

        // Blocks inside any natural loop: last-use move is forbidden there (a static "last use"
        // ordinal is wrong across iterations).
        let loop_blocks: BTreeSet<BlockId> = cfg::natural_loops(func)
            .into_iter()
            .flat_map(|l| l.body)
            .collect();
        let live_out = liveness::live_out(func);

        // Precompute move sites before mutating (needs full stmt lists + shared borrow of `func`).
        let mut move_sites: HashSet<(usize, usize)> = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            if loop_blocks.contains(&BlockId(bi as u32)) {
                continue;
            }
            for (si, stmt) in block.stmts.iter().enumerate() {
                let Statement::Assign(Place::Local(dest), rvalue) = stmt else {
                    continue;
                };
                if !is_owned_ref(dest.0) || !is_borrowed_copy(rvalue, interner) {
                    continue;
                }
                if rvalue_reads_local(rvalue, dest.0) {
                    continue;
                }
                let Some(src) = move_source(rvalue, &is_owned_ref) else {
                    continue;
                };
                if !live_after_stmt(func, &live_out, bi, si, src.0) {
                    move_sites.insert((bi, si));
                }
            }
        }

        // Rule 1: local-assignment RC (release previous occupant, retain borrowed copies). When the
        // new value depends on the *old* one (e.g. `list = Cons(i, list)`), the old value must be
        // released *after* the rvalue is evaluated (the rvalue's container store retains it), not
        // before — otherwise a `+0` old value is freed and then reused mid-evaluation. Such cases
        // stash the old pointer in a synthetic temp and release it after the store.
        //
        // Last-use move: for `dest = src` where `src` is an owned ref local, the site is outside
        // every natural loop, and `src` is dead after the copy — skip Retain and null `src` so the
        // existing +1 transfers to `dest`. Never move from parameters or non-local places.
        let local_types: Vec<dream_types::TypeId> = func.locals.iter().map(|d| d.ty).collect();
        let mut extra_locals: Vec<LocalDecl> = Vec::new();
        let temp_base = func.locals.len() as u32;
        for (bi, block) in func.blocks.iter_mut().enumerate() {
            let mut out: Vec<Statement> = Vec::with_capacity(block.stmts.len());
            for (si, stmt) in block.stmts.drain(..).enumerate() {
                let ref_dest = match &stmt {
                    Statement::Assign(Place::Local(dest), rvalue) if is_owned_ref(dest.0) => {
                        Some((
                            *dest,
                            is_borrowed_copy(rvalue, interner),
                            rvalue_reads_local(rvalue, dest.0),
                            move_source(rvalue, &is_owned_ref),
                        ))
                    }
                    _ => None,
                };
                match ref_dest {
                    Some((dest, retain, true, _)) => {
                        assert!(
                            is_owned_ref(dest.0),
                            "RC insertion on non-owned reference local"
                        );
                        let tmp = Local(temp_base + extra_locals.len() as u32);
                        extra_locals.push(LocalDecl {
                            ty: local_types[dest.0 as usize],
                            name: None,
                            is_ref: false,
                            is_take: false,
                            manual_drop: false,
                        });
                        out.push(Statement::Assign(
                            Place::Local(tmp),
                            Rvalue::Use(Operand::Copy(Place::Local(dest))),
                        ));
                        out.push(stmt);
                        if retain {
                            out.push(Statement::Retain(Operand::Copy(Place::Local(dest))));
                        }
                        out.push(Statement::Release(Operand::Copy(Place::Local(tmp))));
                        changed = true;
                    }
                    Some((dest, retain, false, move_from)) => {
                        out.push(Statement::Release(Operand::Copy(Place::Local(dest))));
                        out.push(stmt);
                        if retain && move_sites.contains(&(bi, si)) {
                            let src = move_from.expect("move site implies owned local source");
                            out.push(Statement::Assign(
                                Place::Local(src),
                                Rvalue::Use(Operand::Const(Const::Null)),
                            ));
                            changed = true;
                        } else if retain {
                            out.push(Statement::Retain(Operand::Copy(Place::Local(dest))));
                            changed = true;
                        } else {
                            changed = true;
                        }
                    }
                    None => out.push(stmt),
                }
            }
            block.stmts = out;
        }
        // Before calls with `take` parameters: retain borrowed RC args so the callee receives a +1.
        // After: null owned locals that transferred their +1 into the callee.
        for block in &mut func.blocks {
            let mut out: Vec<Statement> = Vec::with_capacity(block.stmts.len() + 4);
            for stmt in block.stmts.drain(..) {
                let retains = take_arg_retains(&stmt, &is_owned_ref, &local_is_ref);
                let nulls = take_arg_nulls(&stmt, &is_owned_ref);
                for r in retains {
                    out.push(r);
                    changed = true;
                }
                out.push(stmt);
                for n in nulls {
                    out.push(n);
                    changed = true;
                }
            }
            block.stmts = out;
        }
        // Synthetic old-value temps are pure aliases used only for the deferred release; they must not
        // be released again at scope exit (they are beyond `local_is_ref`, so `is_owned_ref` already
        // excludes them from Rule 3 below).
        func.locals.extend(extra_locals);

        // Rule 3: scope-exit release at every `Return`.
        let owned_locals: Vec<u32> = (0..func.locals.len() as u32)
            .filter(|i| is_owned_ref(*i))
            .collect();
        let ret_is_ref = interner.is_rc_tracked(func.ret);
        let mut spills: Vec<LocalDecl> = Vec::new();
        let next_local = func.locals.len() as u32;
        for block in &mut func.blocks {
            let Terminator::Return(ret) = &block.terminator else {
                continue;
            };
            let (skip, spill_from): (Option<u32>, Option<Operand>) = match ret {
                Some(Operand::Copy(Place::Local(l))) if is_owned_ref(l.0) => (Some(l.0), None),
                Some(op) if ret_is_ref => (None, Some(op.clone())),
                _ => (None, None),
            };
            let skip = if let Some(op) = spill_from {
                let temp = Local(next_local + spills.len() as u32);
                spills.push(LocalDecl {
                    ty: func.ret,
                    name: None,
                    is_ref: false,
                    is_take: false,
                    manual_drop: false,
                });
                block
                    .stmts
                    .push(Statement::Assign(Place::Local(temp), Rvalue::Use(op)));
                block
                    .stmts
                    .push(Statement::Retain(Operand::Copy(Place::Local(temp))));
                block.terminator = Terminator::Return(Some(Operand::Copy(Place::Local(temp))));
                changed = true;
                Some(temp.0)
            } else {
                skip
            };
            for &i in &owned_locals {
                if Some(i) == skip {
                    continue;
                }
                block
                    .stmts
                    .push(Statement::Release(Operand::Copy(Place::Local(Local(i)))));
                changed = true;
            }
        }
        func.locals.extend(spills);
        changed
    }
}

/// If `rvalue` is a plain copy of an owned reference local, return that local (candidate for move).
fn move_source(rvalue: &Rvalue, is_owned_ref: &dyn Fn(u32) -> bool) -> Option<Local> {
    match rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) if is_owned_ref(src.0) => Some(*src),
        _ => None,
    }
}

/// Null locals transferred into `take` parameters of a call (ownership moves to the callee).
fn take_arg_nulls(stmt: &Statement, is_owned_ref: &dyn Fn(u32) -> bool) -> Vec<Statement> {
    let (take_params, args): (&[bool], &[Operand]) = match stmt {
        Statement::Call { callee, args } => (&callee.take_params, args),
        Statement::Assign(_, Rvalue::Call { callee, args, .. }) => (&callee.take_params, args),
        Statement::Assign(_, Rvalue::New { .. }) => return Vec::new(),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        if let Operand::Copy(Place::Local(l)) = arg {
            if is_owned_ref(l.0) {
                out.push(Statement::Assign(
                    Place::Local(*l),
                    Rvalue::Use(Operand::Const(Const::Null)),
                ));
            }
        }
    }
    out
}

/// Retain borrowed RC values passed to `take` parameters so the callee still receives a +1.
/// Owned locals transfer their existing +1 (see [`take_arg_nulls`]) and must not be retained here.
fn take_arg_retains(
    stmt: &Statement,
    is_owned_ref: &dyn Fn(u32) -> bool,
    local_is_ref: &[bool],
) -> Vec<Statement> {
    let (take_params, args): (&[bool], &[Operand]) = match stmt {
        Statement::Call { callee, args } => (&callee.take_params, args),
        Statement::Assign(_, Rvalue::Call { callee, args, .. }) => (&callee.take_params, args),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        match arg {
            Operand::Copy(Place::Local(l))
                if local_is_ref.get(l.0 as usize).copied().unwrap_or(false)
                    && !is_owned_ref(l.0) =>
            {
                out.push(Statement::Retain(Operand::Copy(Place::Local(*l))));
            }
            Operand::Copy(Place::Field { .. })
            | Operand::Copy(Place::Index { .. })
            | Operand::Const(Const::Str(_)) => {
                out.push(Statement::Retain(arg.clone()));
            }
            _ => {}
        }
    }
    out
}
