//! [`RcInsertion`]: make reference ownership explicit in MIR.

use super::liveness::{self, live_after_stmt, live_in_of, stmt_reads_local};
use super::{is_borrowed_copy, rvalue_reads_local};
use crate::passes::MirPass;
use crate::{Const, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{HashMap, HashSet};

pub struct RcInsertion;

impl MirPass for RcInsertion {
    fn name(&self) -> &'static str {
        "rc-insertion"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        // Infer non-owning aliases before ownership insertion so field/index loads skip retain.
        super::cursor::infer_cursors(func, interner);

        let local_is_ref: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.is_rc_tracked(d.ty))
            .collect();
        let local_is_cursor: Vec<bool> = func.locals.iter().map(|d| d.is_cursor).collect();
        let params: HashSet<u32> = func.params.iter().map(|p| p.0).collect();
        let take_params: HashSet<u32> = func
            .params
            .iter()
            .copied()
            .filter(|p| func.locals[p.0 as usize].is_take)
            .map(|p| p.0)
            .collect();
        // Take params own their +1 (like ordinary owned locals). Ordinary params are borrowed.
        // Cursors never own.
        let is_owned_ref = |l: u32| {
            local_is_ref.get(l as usize).copied().unwrap_or(false)
                && !local_is_cursor.get(l as usize).copied().unwrap_or(false)
                && (!params.contains(&l) || take_params.contains(&l))
        };
        let mut changed = false;

        let live_out = liveness::live_out(func);

        // Last-use move: for `dest = src` where `src` is an owned ref local and `src` is dead after
        // the copy (CFG liveness, including loop back-edges) — skip Retain and null `src` so the
        // existing +1 transfers to `dest`. Never move from parameters or non-local places.
        let mut move_sites: HashSet<(usize, usize)> = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
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
        // Last-use move: for `dest = src` where `src` is an owned ref local and `src` is dead after
        // the copy — skip Retain and null `src` so the existing +1 transfers to `dest`.
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
                            is_cursor: false,
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
        // Sink-call ABI (Nim-style): callee always receives +1.
        // - Last use of an owned local → move (null source, no retain).
        // - Still-live owned local → retain a copy, keep the caller's binding.
        // - Borrowed / non-owned RC → retain into the sink.
        // Recompute liveness after assign-RC rewrites above.
        let live_out_calls = liveness::live_out(func);
        let mut take_moves: HashSet<(usize, usize, u32)> = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                for local in take_owned_arg_locals(stmt, &is_owned_ref) {
                    if !live_after_stmt(func, &live_out_calls, bi, si, local) {
                        take_moves.insert((bi, si, local));
                    }
                }
            }
        }
        for (bi, block) in func.blocks.iter_mut().enumerate() {
            let mut out: Vec<Statement> = Vec::with_capacity(block.stmts.len() + 4);
            for (si, stmt) in block.stmts.drain(..).enumerate() {
                let (retains, nulls) =
                    take_arg_effects(&stmt, &is_owned_ref, &local_is_ref, |local| {
                        take_moves.contains(&(bi, si, local))
                    });
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

        // Last-use destroy: Release+null after the last statement that uses an owned RC local, so
        // UI temps and `js` handles unpin before later unrelated work. Scope-exit Release at Return
        // stays (nop on null). Skip terminator-only last uses (`if x`).
        //
        // Functions with `Await` keep RC locals until resume / `AsyncComplete`: a capture cell or
        // funcbox can be CFG-dead after the call that produces the awaited future while the host
        // worker still holds `funcbox_env` as a raw pointer. Releasing before suspend UAF-hangs
        // `WebWorkerPool.dispatch_async`. Sync functions (UI rebuild) still destroy at last use.
        // Sink/take params are excluded: early `= null` on a remapped param would copy-prop into the
        // caller's still-live argument after inlining.
        let has_await = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Await { .. }));
        if !has_await {
            insert_early_releases(func, interner, &is_owned_ref, &take_params, &mut changed);
            insert_early_value_drops(func, interner, &mut changed);
        }

        // Rule 3: scope-exit release at every `Return` / `AsyncComplete`. Await must not release —
        // coroutine locals stay live across suspend (ownership moves to the Future frame).
        let owned_locals: Vec<u32> = (0..func.locals.len() as u32)
            .filter(|i| is_owned_ref(*i))
            .collect();
        let ret_is_ref = interner.is_rc_tracked(func.ret);
        let mut spills: Vec<LocalDecl> = Vec::new();
        let next_local = func.locals.len() as u32;
        for block in &mut func.blocks {
            let ret = match &block.terminator {
                Terminator::Return(v) | Terminator::AsyncComplete(v) => v.clone(),
                _ => continue,
            };
            let is_async_complete = matches!(block.terminator, Terminator::AsyncComplete(_));
            let (skip, spill_from): (Option<u32>, Option<Operand>) = match &ret {
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
                    is_cursor: false,
                    manual_drop: false,
                });
                block
                    .stmts
                    .push(Statement::Assign(Place::Local(temp), Rvalue::Use(op)));
                block
                    .stmts
                    .push(Statement::Retain(Operand::Copy(Place::Local(temp))));
                let spilled = Some(Operand::Copy(Place::Local(temp)));
                block.terminator = if is_async_complete {
                    Terminator::AsyncComplete(spilled)
                } else {
                    Terminator::Return(spilled)
                };
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

fn release_and_null(local: u32) -> [Statement; 2] {
    [
        Statement::Release(Operand::Copy(Place::Local(Local(local)))),
        Statement::Assign(
            Place::Local(Local(local)),
            Rvalue::Use(Operand::Const(Const::Null)),
        ),
    ]
}

fn rc_op_on_local(stmt: &Statement, local: u32) -> bool {
    match stmt {
        Statement::Retain(Operand::Copy(Place::Local(l)))
        | Statement::Release(Operand::Copy(Place::Local(l))) => l.0 == local,
        Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null))) => {
            l.0 == local
        }
        _ => false,
    }
}

fn assigns_local(stmt: &Statement, local: u32) -> bool {
    matches!(stmt, Statement::Assign(Place::Local(l), _) if l.0 == local)
}

/// Last-use destroy is for pinning (`js` / class temps after a read or DOM op), not for
/// arguments of `Call`/`New`/`UnionNew`: those callees may store a borrow (funcbox env,
/// weak field payload, union spine) that CFG liveness does not see.
fn allows_early_destroy(stmt: &Statement) -> bool {
    match stmt {
        Statement::Print { .. } | Statement::JsCall { .. } => true,
        Statement::Assign(_, rv) => matches!(
            rv,
            Rvalue::Use(_)
                | Rvalue::Select { .. }
                | Rvalue::Unary(_, _)
                | Rvalue::Binary(_, _, _)
                | Rvalue::UnionField { .. }
                | Rvalue::ToString(_)
                | Rvalue::Concat(_, _)
                | Rvalue::StrLen(_)
                | Rvalue::StrByteSize(_)
                | Rvalue::CharAt(_, _)
                | Rvalue::ByteAt(_, _)
                | Rvalue::HashCode(_)
        ),
        _ => false,
    }
}

/// Last statement index that still belongs to the same source line as `si`.
///
/// `println(x.id)` lowers to a field load plus a print; `del` must wait until that line finishes.
/// A later [`Statement::SourceLine`] ends the line (so later work in the same function can run
/// after destroy). With no later marker, the rest of the block is the current line — typical for
/// a helper whose last statement is that print, right before `Return`.
fn source_line_end(block: &crate::BasicBlock, si: usize) -> usize {
    let mut end = si;
    for (j, s) in block.stmts.iter().enumerate().skip(si + 1) {
        if matches!(s, Statement::SourceLine(_)) {
            break;
        }
        end = j;
    }
    end
}

/// Last-use destroy for owned RC locals (classes, strings, collections, `js`).
fn insert_early_releases(
    func: &mut MirFunction,
    interner: &TypeInterner,
    is_owned_ref: &dyn Fn(u32) -> bool,
    take_params: &HashSet<u32>,
    changed: &mut bool,
) {
    let live_out = liveness::live_out(func);
    let live_in: Vec<HashSet<u32>> = (0..func.blocks.len())
        .map(|bi| live_in_of(func, &live_out, bi))
        .collect();
    let owned: Vec<u32> = (0..func.locals.len() as u32)
        .filter(|i| {
            is_owned_ref(*i)
                && !take_params.contains(i)
                && {
                    let ty = func.locals[*i as usize].ty;
                    ty != interner.string()
                        && !matches!(
                            interner.kind(ty),
                            dream_types::TyKind::Array(_)
                                | dream_types::TyKind::Func(_, _)
                                | dream_types::TyKind::Union(_, _)
                        )
                }
        })
        .collect();
    let mut transferred: HashSet<(usize, usize, u32)> = HashSet::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            if let Statement::Assign(Place::Local(_), rvalue) = stmt {
                if is_borrowed_copy(rvalue, interner) {
                    if let Some(src) = move_source(rvalue, is_owned_ref) {
                        if !live_after_stmt(func, &live_out, bi, si, src.0) {
                            transferred.insert((bi, si, src.0));
                        }
                    }
                }
            }
            for local in take_owned_arg_locals(stmt, is_owned_ref) {
                if !live_after_stmt(func, &live_out, bi, si, local) {
                    transferred.insert((bi, si, local));
                }
            }
        }
    }

    let mut die_after: HashSet<(usize, usize, u32)> = HashSet::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            for &local in &owned {
                if transferred.contains(&(bi, si, local)) || rc_op_on_local(stmt, local) {
                    continue;
                }
                let used = stmt_reads_local(stmt, local);
                let defined_dead =
                    assigns_local(stmt, local) && !live_after_stmt(func, &live_out, bi, si, local);
                if !used && !defined_dead {
                    continue;
                }
                if used && live_after_stmt(func, &live_out, bi, si, local) {
                    continue;
                }
                if defined_dead || (used && !live_after_stmt(func, &live_out, bi, si, local)) {
                    if !allows_early_destroy(stmt) {
                        continue;
                    }
                    // Delay until the end of the source line so `del` does not run mid-expression
                    // (`println(x.id)` is a field load plus a print).
                    die_after.insert((bi, source_line_end(block, si), local));
                }
            }
        }
    }

    for bi in 0..func.blocks.len() {
        let mut out: Vec<Statement> = Vec::with_capacity(func.blocks[bi].stmts.len() + 4);
        for (si, stmt) in func.blocks[bi].stmts.drain(..).enumerate() {
            out.push(stmt);
            for &local in &owned {
                if die_after.contains(&(bi, si, local)) {
                    out.extend(release_and_null(local));
                    *changed = true;
                }
            }
        }
        if let Terminator::Await { future, .. } = &func.blocks[bi].terminator {
            let future_local = match future {
                Operand::Copy(Place::Local(l)) => Some(l.0),
                _ => None,
            };
            for &local in &owned {
                if Some(local) == future_local {
                    continue;
                }
                if !live_in[bi].contains(&local) || live_out[bi].contains(&local) {
                    continue;
                }
                let already = out.iter().any(|s| {
                    matches!(
                        s,
                        Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                            if l.0 == local
                    )
                });
                if already {
                    continue;
                }
                out.extend(release_and_null(local));
                *changed = true;
            }
        }
        func.blocks[bi].stmts = out;
    }

    // Futures that die after resume: release at the start of the resume block (still needed at Await).
    let live_out = liveness::live_out(func);
    let mut resume_releases: Vec<(usize, u32)> = Vec::new();
    for block in &func.blocks {
        if let Terminator::Await { future, resume, .. } = &block.terminator {
            let Operand::Copy(Place::Local(l)) = future else {
                continue;
            };
            if !is_owned_ref(l.0) {
                continue;
            }
            if live_in_of(func, &live_out, resume.0 as usize).contains(&l.0) {
                continue;
            }
            resume_releases.push((resume.0 as usize, l.0));
        }
    }
    for (ri, local) in resume_releases {
        let already = func.blocks[ri]
            .stmts
            .iter()
            .any(|s| rc_op_on_local(s, local));
        if already {
            continue;
        }
        let mut stmts = Vec::with_capacity(func.blocks[ri].stmts.len() + 2);
        stmts.extend(release_and_null(local));
        stmts.append(&mut func.blocks[ri].stmts);
        func.blocks[ri].stmts = stmts;
        *changed = true;
    }
}

fn is_owning_value_local(func: &MirFunction, interner: &TypeInterner, idx: usize) -> bool {
    let decl = &func.locals[idx];
    if !interner.is_value_type(decl.ty) || decl.is_ref || decl.manual_drop {
        return false;
    }
    if decl.name.as_deref() == Some("this") {
        return false;
    }
    if idx < func.params.len() {
        return false;
    }
    if decl.name.is_some() {
        return true;
    }
    false
}

/// Early `ValueDrop` after the last use of an owning value local. Whole-value copy-out (`dest = src`)
/// keeps copy semantics (frame teardown still drops `src`); last *read* drops immediately.
fn insert_early_value_drops(func: &mut MirFunction, interner: &TypeInterner, changed: &mut bool) {
    let live_out = liveness::live_out(func);
    let owning: Vec<u32> = (0..func.locals.len())
        .filter(|&i| is_owning_value_local(func, interner, i))
        .map(|i| i as u32)
        .collect();
    if owning.is_empty() {
        return;
    }
    let mut drop_at: Vec<(usize, usize, u32)> = Vec::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            for &local in &owning {
                if matches!(stmt, Statement::ValueDrop(l) if l.0 == local) {
                    continue;
                }
                let whole_copy_out = matches!(
                    stmt,
                    Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s))))
                        if s.0 == local && d.0 != local
                );
                if whole_copy_out {
                    continue;
                }
                let used = stmt_reads_local(stmt, local) || assigns_local(stmt, local);
                if !used || live_after_stmt(func, &live_out, bi, si, local) {
                    continue;
                }
                drop_at.push((bi, source_line_end(block, si), local));
            }
        }
    }
    if drop_at.is_empty() {
        return;
    }
    let mut by_block: HashMap<usize, Vec<(usize, u32)>> = HashMap::new();
    for (bi, si, local) in drop_at {
        by_block.entry(bi).or_default().push((si, local));
        func.locals[local as usize].manual_drop = true;
    }
    for (bi, sites) in by_block {
        let mut out: Vec<Statement> = Vec::with_capacity(func.blocks[bi].stmts.len() + sites.len());
        for (si, stmt) in func.blocks[bi].stmts.drain(..).enumerate() {
            out.push(stmt);
            for (ssi, local) in &sites {
                if *ssi == si {
                    out.push(Statement::ValueDrop(Local(*local)));
                    *changed = true;
                }
            }
        }
        func.blocks[bi].stmts = out;
    }
}

/// If `rvalue` is a plain copy (or equivalent upcast) of an owned reference local, return that local.
fn move_source(rvalue: &Rvalue, is_owned_ref: &dyn Fn(u32) -> bool) -> Option<Local> {
    match rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) if is_owned_ref(src.0) => Some(*src),
        _ => None,
    }
}

/// Sink-arg sites: ordinary calls use [`Callee::take_params`]; `New` with a user constructor sinks
/// every argument (ctor params are sink-default and `this` is not in `args`).
fn sink_call_args(stmt: &Statement) -> Option<(Vec<bool>, &[Operand])> {
    match stmt {
        Statement::Call { callee, args } => Some((callee.take_params.clone(), args)),
        Statement::Assign(_, Rvalue::Call { callee, args, .. }) => {
            Some((callee.take_params.clone(), args))
        }
        // Constructor payloads are unmarked sinks; without this, callers release after `New` while
        // the ctor already moved the same +1 into fields (UAF / empty JsonValue maps, etc.).
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(_),
                args,
                ..
            },
        ) => Some((vec![true; args.len()], args)),
        // Indirect `fun` values carry no `take_params` ABI, but async constructors (and other
        // sink-default callees) still consume the argument. Treat every indirect arg as a sink so
        // the wrapper does not release a pointer the callee stored on a Future frame.
        Statement::IndirectCall { args, .. } => Some((vec![true; args.len()], args)),
        Statement::Assign(_, Rvalue::IndirectCall { args, .. }) => {
            Some((vec![true; args.len()], args))
        }
        _ => None,
    }
}

/// Sink-arg effects at a call: retains (copies into the callee's +1) and nulls (moves).
/// `is_move` is true when this owned local is a last-use transfer into the sink.
fn take_arg_effects(
    stmt: &Statement,
    is_owned_ref: &dyn Fn(u32) -> bool,
    local_is_ref: &[bool],
    is_move: impl Fn(u32) -> bool,
) -> (Vec<Statement>, Vec<Statement>) {
    let Some((take_params, args)) = sink_call_args(stmt) else {
        return (Vec::new(), Vec::new());
    };
    let mut retains = Vec::new();
    let mut nulls = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        match arg {
            Operand::Copy(Place::Local(l))
                if local_is_ref.get(l.0 as usize).copied().unwrap_or(false) =>
            {
                if is_owned_ref(l.0) && is_move(l.0) {
                    nulls.push(Statement::Assign(
                        Place::Local(*l),
                        Rvalue::Use(Operand::Const(Const::Null)),
                    ));
                } else {
                    retains.push(Statement::Retain(Operand::Copy(Place::Local(*l))));
                }
            }
            Operand::Copy(Place::Field { .. })
            | Operand::Copy(Place::Index { .. })
            | Operand::Const(Const::Str(_)) => {
                retains.push(Statement::Retain(arg.clone()));
            }
            _ => {}
        }
    }
    (retains, nulls)
}

fn take_owned_arg_locals(stmt: &Statement, is_owned_ref: &dyn Fn(u32) -> bool) -> Vec<u32> {
    let Some((take_params, args)) = sink_call_args(stmt) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        if let Operand::Copy(Place::Local(l)) = arg {
            if is_owned_ref(l.0) {
                out.push(l.0);
            }
        }
    }
    out
}
