//! Null out GC-tracked locals after their last use so the root table does not keep them alive
//! across later `Debug.gc_collect()` / nursery collections (including when scopes were inlined).

use super::{MirFunction, MirPass};
use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{TyKind, TypeInterner};
use std::collections::HashSet;

pub struct ClearDeadGcRoots;

impl MirPass for ClearDeadGcRoots {
    fn name(&self) -> &'static str {
        "clear-dead-gc-roots"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let gc_locals: HashSet<u32> = func
            .locals
            .iter()
            .enumerate()
            .filter(|(_, d)| interner.is_gc_tracked(d.ty))
            .map(|(i, _)| i as u32)
            .collect();
        if gc_locals.is_empty() {
            return false;
        }

        let n = func.blocks.len();
        let mut live_in: Vec<HashSet<u32>> = vec![HashSet::new(); n];
        let mut live_out: Vec<HashSet<u32>> = vec![HashSet::new(); n];

        let mut changed = true;
        while changed {
            changed = false;
            for bi in (0..n).rev() {
                let mut out = HashSet::new();
                for succ in successors(&func.blocks[bi].terminator) {
                    out.extend(live_in[succ].iter().copied());
                }
                if out != live_out[bi] {
                    live_out[bi] = out;
                    changed = true;
                }
                let mut inn = live_out[bi].clone();
                for u in terminator_uses(&func.blocks[bi].terminator) {
                    inn.insert(u);
                }
                for stmt in func.blocks[bi].stmts.iter().rev() {
                    for d in stmt_defs(stmt) {
                        inn.remove(&d);
                    }
                    for u in stmt_uses(stmt) {
                        inn.insert(u);
                    }
                }
                if inn != live_in[bi] {
                    live_in[bi] = inn;
                    changed = true;
                }
            }
        }

        let mut modified = false;

        // Params that are never used are dead on entry — null them so Js handles unregister.
        let param_count = func.params.len() as u32;
        if !func.blocks.is_empty() {
            let mut clears = Vec::new();
            for l in 0..param_count {
                if gc_locals.contains(&l) && !live_in[0].contains(&l) {
                    clears.push(Statement::Assign(
                        Place::Local(crate::Local(l)),
                        Rvalue::Use(Operand::Const(Const::Null)),
                    ));
                    modified = true;
                }
            }
            if !clears.is_empty() {
                let stmts = std::mem::take(&mut func.blocks[0].stmts);
                let mut out = clears;
                out.extend(stmts);
                func.blocks[0].stmts = out;
            }
        }

        // `funcidx`/`env` are unmanaged ints; the box must stay rooted until every `await` in this
        // function has settled (the last use is often in a block *before* the `Await`).
        let pin_fun_across_await = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Await { .. }));

        #[allow(clippy::needless_range_loop)] // indexes `live_out` and `func.blocks` together
        for bi in 0..n {
            let stmts = std::mem::take(&mut func.blocks[bi].stmts);
            let mut live = live_out[bi].clone();
            for u in terminator_uses(&func.blocks[bi].terminator) {
                live.insert(u);
            }
            let mut live_after = vec![HashSet::new(); stmts.len()];
            for i in (0..stmts.len()).rev() {
                live_after[i] = live.clone();
                for d in stmt_defs(&stmts[i]) {
                    live.remove(&d);
                }
                for u in stmt_uses(&stmts[i]) {
                    live.insert(u);
                }
            }

            let mut out_stmts = Vec::with_capacity(stmts.len() * 2);
            for (i, stmt) in stmts.into_iter().enumerate() {
                let used: Vec<u32> = stmt_uses(&stmt)
                    .into_iter()
                    .filter(|l| gc_locals.contains(l))
                    .collect();
                let defined: Vec<u32> = stmt_defs(&stmt)
                    .into_iter()
                    .filter(|l| gc_locals.contains(l))
                    .collect();
                out_stmts.push(stmt);
                // Null after the last use, and also after a def whose value is never used again
                // (common after inlining: a callee-local is assigned then the inlined region ends
                // without a use, and the same local is reused later — without this hole the old
                // pointer stays in the root table across `Debug.gc_collect()`).
                let mut to_clear: Vec<u32> = used
                    .into_iter()
                    .filter(|l| !live_after[i].contains(l))
                    .collect();
                for d in defined {
                    if !live_after[i].contains(&d) && !to_clear.contains(&d) {
                        to_clear.push(d);
                    }
                }
                for l in to_clear {
                    if pin_fun_across_await
                        && matches!(
                            interner.kind(func.locals[l as usize].ty),
                            TyKind::Func(..)
                        )
                    {
                        continue;
                    }
                    out_stmts.push(Statement::Assign(
                        Place::Local(crate::Local(l)),
                        Rvalue::Use(Operand::Const(Const::Null)),
                    ));
                    modified = true;
                }
            }
            func.blocks[bi].stmts = out_stmts;
        }
        modified
    }
}

fn successors(t: &Terminator) -> Vec<usize> {
    match t {
        Terminator::Goto(b) => vec![b.0 as usize],
        Terminator::If {
            then_blk, else_blk, ..
        } => vec![then_blk.0 as usize, else_blk.0 as usize],
        Terminator::Switch {
            targets, default, ..
        } => {
            let mut v: Vec<usize> = targets.iter().map(|(_, b)| b.0 as usize).collect();
            v.push(default.0 as usize);
            v
        }
        Terminator::Await { resume, .. } => vec![resume.0 as usize],
        Terminator::Return(_)
        | Terminator::TailCall { .. }
        | Terminator::Unreachable
        | Terminator::AsyncComplete(_) => vec![],
    }
}

fn place_local(p: &Place) -> Option<u32> {
    match p {
        Place::Local(l) => Some(l.0),
        Place::Field { base, .. } | Place::Index { base, .. } => Some(base.0),
        Place::Deref { ptr, .. } => Some(ptr.0),
        Place::Global(_) => None,
    }
}

fn operand_local(o: &Operand) -> Option<u32> {
    match o {
        Operand::Copy(p) => place_local(p),
        Operand::Const(_) => None,
    }
}

fn stmt_defs(s: &Statement) -> Vec<u32> {
    match s {
        Statement::Assign(Place::Local(l), _) => vec![l.0],
        _ => vec![],
    }
}

fn stmt_uses(s: &Statement) -> Vec<u32> {
    let mut u = Vec::new();
    match s {
        Statement::Assign(place, rv) => {
            if let Place::Field { base, .. } | Place::Index { base, .. } = place {
                u.push(base.0);
            }
            if let Place::Deref { ptr, .. } = place {
                u.push(ptr.0);
            }
            rvalue_uses(rv, &mut u);
        }
        Statement::Panic(o)
        | Statement::ForceFree(o)
        | Statement::LockAcquire(o)
        | Statement::LockRelease(o) => {
            if let Some(l) = operand_local(o) {
                u.push(l);
            }
        }
        Statement::Print { arg, .. } => {
            if let Some(l) = operand_local(arg) {
                u.push(l);
            }
        }
        Statement::Call { args, .. } => {
            for a in args {
                if let Some(l) = operand_local(a) {
                    u.push(l);
                }
            }
        }
        Statement::IndirectCall { target, args, .. } => {
            if let Some(l) = operand_local(target) {
                u.push(l);
            }
            for a in args {
                if let Some(l) = operand_local(a) {
                    u.push(l);
                }
            }
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            if let Some(l) = operand_local(receiver) {
                u.push(l);
            }
            for a in args {
                if let Some(l) = operand_local(a) {
                    u.push(l);
                }
            }
        }
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            if let Some(l) = operand_local(target) {
                u.push(l);
            }
            if let Some(v) = via {
                if let Some(l) = operand_local(v) {
                    u.push(l);
                }
            }
            if let Some(m) = method {
                if let Some(l) = operand_local(m) {
                    u.push(l);
                }
            }
            for (a, _) in args {
                if let Some(l) = operand_local(a) {
                    u.push(l);
                }
            }
        }
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            for o in [dst, dst_off, src, src_off, count] {
                if let Some(l) = operand_local(o) {
                    u.push(l);
                }
            }
        }
        Statement::ValueDrop(local) => u.push(local.0),
        Statement::SimdF32x4 {
            dest,
            lhs,
            rhs,
            index,
            ..
        } => {
            for o in [dest, lhs, rhs, index] {
                if let Some(l) = operand_local(o) {
                    u.push(l);
                }
            }
        }
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
    }
    u
}

fn rvalue_uses(rv: &Rvalue, u: &mut Vec<u32>) {
    let mut push = |o: &Operand| {
        if let Some(l) = operand_local(o) {
            u.push(l);
        }
    };
    match rv {
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::Discriminant(o)
        | Rvalue::IsType(o, _)
        | Rvalue::Cast(o, _, _)
        | Rvalue::EnumName { value: o, .. } => push(o),
        Rvalue::Binary(_, a, b)
        | Rvalue::CharAt(a, b)
        | Rvalue::ByteAt(a, b)
        | Rvalue::Concat(a, b) => {
            push(a);
            push(b);
        }
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            push(cond);
            push(then_val);
            push(else_val);
        }
        Rvalue::ArrayNew { len, .. } => push(len),
        Rvalue::Call { args, .. } => {
            for a in args {
                push(a);
            }
        }
        Rvalue::IndirectCall { target, args, .. } => {
            push(target);
            for a in args {
                push(a);
            }
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            push(receiver);
            for a in args {
                push(a);
            }
        }
        Rvalue::FuncRef(_) => {}
        Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::Tuple { elems: args, .. }
        | Rvalue::ArrayLit { elems: args, .. } => {
            for a in args {
                push(a);
            }
        }
        Rvalue::ToBytes { value, .. } => push(value),
        Rvalue::FromBytes { bytes, .. } => push(bytes),
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            push(array);
            push(new_len);
        }
        Rvalue::UnionField { base, .. } => push(base),
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            push(target);
            if let Some(v) = via {
                push(v);
            }
            if let Some(m) = method {
                push(m);
            }
            for (a, _) in args {
                push(a);
            }
        }
    }
}

fn terminator_uses(t: &Terminator) -> Vec<u32> {
    match t {
        Terminator::If { cond, .. } => operand_local(cond).into_iter().collect(),
        Terminator::Switch { value, .. } => operand_local(value).into_iter().collect(),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            operand_local(o).into_iter().collect()
        }
        Terminator::Await { future, dest, .. } => {
            let mut v = operand_local(future).into_iter().collect::<Vec<_>>();
            if let Some(d) = dest {
                v.push(d.0);
            }
            v
        }
        Terminator::TailCall { args, .. } => args.iter().filter_map(operand_local).collect(),
        _ => vec![],
    }
}
