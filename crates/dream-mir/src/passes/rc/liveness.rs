//! Backward liveness of locals, used by last-use move in [`super::RcInsertion`].

use crate::{MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use std::collections::HashSet;

/// Per-block live-out set: locals that may be read on some path after the block's terminator.
pub(crate) fn live_out(func: &MirFunction) -> Vec<HashSet<u32>> {
    let n = func.blocks.len();
    let mut live_out = vec![HashSet::new(); n];
    let mut live_in = vec![HashSet::new(); n];
    let mut changed = true;
    while changed {
        changed = false;
        for bi in (0..n).rev() {
            let mut out = HashSet::new();
            for succ in func.blocks[bi].terminator.successors() {
                out.extend(&live_in[succ.0 as usize]);
            }
            if out != live_out[bi] {
                live_out[bi] = out;
                changed = true;
            }
            let mut inn = live_out[bi].clone();
            transfer_block(
                &func.blocks[bi].stmts,
                &func.blocks[bi].terminator,
                &mut inn,
            );
            if inn != live_in[bi] {
                live_in[bi] = inn;
                changed = true;
            }
        }
    }
    live_out
}

/// Locals live at the start of block `bi` (may be read in the block or after it).
pub(crate) fn live_in_of(func: &MirFunction, live_out: &[HashSet<u32>], bi: usize) -> HashSet<u32> {
    let block = &func.blocks[bi];
    let mut inn = live_out[bi].clone();
    transfer_block(&block.stmts, &block.terminator, &mut inn);
    inn
}

/// True if `local` may be read after statement `si` in block `bi` (not counting uses inside that
/// statement itself).
pub(crate) fn live_after_stmt(
    func: &MirFunction,
    live_out: &[HashSet<u32>],
    bi: usize,
    si: usize,
    local: u32,
) -> bool {
    let block = &func.blocks[bi];
    let mut live = live_out[bi].clone();
    add_terminator_reads(&block.terminator, &mut live);
    for stmt in block.stmts[si + 1..].iter().rev() {
        transfer_stmt(stmt, &mut live);
    }
    live.contains(&local)
}

fn transfer_block(stmts: &[Statement], term: &Terminator, live: &mut HashSet<u32>) {
    add_terminator_reads(term, live);
    for stmt in stmts.iter().rev() {
        transfer_stmt(stmt, live);
    }
}

fn transfer_stmt(stmt: &Statement, live: &mut HashSet<u32>) {
    match stmt {
        Statement::Assign(Place::Local(d), rv) => {
            live.remove(&d.0);
            add_rvalue_reads(rv, live);
        }
        Statement::Assign(place, rv) => {
            add_place_base_reads(place, live);
            add_rvalue_reads(rv, live);
        }
        Statement::Retain(op) | Statement::Release(op) | Statement::Panic(op) => {
            add_operand_reads(op, live)
        }
        Statement::Call { args, .. } => args.iter().for_each(|a| add_operand_reads(a, live)),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            add_operand_reads(target, live);
            if let Some(v) = via {
                add_operand_reads(v, live);
            }
            if let Some(m) = method {
                add_operand_reads(m, live);
            }
            args.iter().for_each(|(a, _)| add_operand_reads(a, live));
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            add_operand_reads(receiver, live);
            args.iter().for_each(|a| add_operand_reads(a, live));
        }
        Statement::IndirectCall { target, args, .. } => {
            add_operand_reads(target, live);
            args.iter().for_each(|a| add_operand_reads(a, live));
        }
        Statement::Print { arg, .. } => add_operand_reads(arg, live),
        Statement::ForceFree(o) => add_operand_reads(o, live),
        Statement::ValueDrop(l) | Statement::ValueRetain(l) | Statement::ValueKill(l) => {
            live.insert(l.0);
        }
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            add_operand_reads(dst, live);
            add_operand_reads(dst_off, live);
            add_operand_reads(src, live);
            add_operand_reads(src_off, live);
            add_operand_reads(count, live);
        }
        Statement::LockAcquire(o) | Statement::LockRelease(o) => add_operand_reads(o, live),
        Statement::SimdF32x4 {
            dest,
            lhs,
            rhs,
            index,
            ..
        } => {
            add_operand_reads(dest, live);
            add_operand_reads(lhs, live);
            add_operand_reads(rhs, live);
            add_operand_reads(index, live);
        }
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
    }
}

/// True if `local` is read by `stmt` (plain operand, field/index base, or RC op).
pub(crate) fn stmt_reads_local(stmt: &Statement, local: u32) -> bool {
    let mut live = HashSet::new();
    transfer_stmt_reads_only(stmt, &mut live);
    live.contains(&local)
}

fn transfer_stmt_reads_only(stmt: &Statement, live: &mut HashSet<u32>) {
    match stmt {
        Statement::Assign(Place::Local(_), rv) => add_rvalue_reads(rv, live),
        Statement::Assign(place, rv) => {
            add_place_base_reads(place, live);
            add_rvalue_reads(rv, live);
        }
        _ => transfer_stmt(stmt, live),
    }
}

fn add_terminator_reads(term: &Terminator, live: &mut HashSet<u32>) {
    match term {
        Terminator::If { cond, .. } => add_operand_reads(cond, live),
        Terminator::Switch { value, .. } => add_operand_reads(value, live),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            add_operand_reads(o, live)
        }
        Terminator::TailCall { args, .. } => args.iter().for_each(|a| add_operand_reads(a, live)),
        Terminator::Await { future, .. } => add_operand_reads(future, live),
        _ => {}
    }
}

fn add_rvalue_reads(rv: &Rvalue, live: &mut HashSet<u32>) {
    let mut add = |op: &Operand| add_operand_reads(op, live);
    match rv {
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            add(cond);
            add(then_val);
            add(else_val);
        }
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::Discriminant(o)
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. } => add(o),
        Rvalue::Binary(_, a, b)
        | Rvalue::CharAt(a, b)
        | Rvalue::ByteAt(a, b)
        | Rvalue::Concat(a, b) => {
            add(a);
            add(b);
        }
        Rvalue::EnumName { value, .. } => add(value),
        Rvalue::ArrayNew { len, .. } => add(len),
        Rvalue::ToBytes { value: o, .. } | Rvalue::FromBytes { bytes: o, .. } => add(o),
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            add(array);
            add(new_len);
        }
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => args.iter().for_each(add),
        Rvalue::IndirectCall { target, args, .. } => {
            add(target);
            args.iter().for_each(add);
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            add(receiver);
            args.iter().for_each(add);
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            add(target);
            if let Some(v) = via {
                add(v);
            }
            if let Some(m) = method {
                add(m);
            }
            args.iter().for_each(|(a, _)| add(a));
        }
        Rvalue::FuncRef(_) => {}
    }
}

fn add_operand_reads(op: &Operand, live: &mut HashSet<u32>) {
    if let Operand::Copy(place) = op {
        match place {
            Place::Local(l) => {
                live.insert(l.0);
            }
            Place::Field { base, .. } => {
                live.insert(base.0);
            }
            Place::Index { base, index, .. } => {
                live.insert(base.0);
                add_operand_reads(index, live);
            }
            Place::Deref { ptr, .. } => {
                live.insert(ptr.0);
            }
            Place::Global(_) => {}
        }
    }
}

fn add_place_base_reads(place: &Place, live: &mut HashSet<u32>) {
    match place {
        Place::Field { base, .. } => {
            live.insert(base.0);
        }
        Place::Index { base, index, .. } => {
            live.insert(base.0);
            add_operand_reads(index, live);
        }
        Place::Deref { ptr, .. } => {
            live.insert(ptr.0);
        }
        Place::Local(_) | Place::Global(_) => {}
    }
}
