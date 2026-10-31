//! Backward liveness of locals, used by last-use move in [`super::RcInsertion`].

use crate::{MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use indexmap::{IndexMap, IndexSet};
use std::collections::BTreeSet;

/// Per-block live-out set: locals that may be read on some path after the block's terminator.
pub(crate) fn live_out(func: &MirFunction) -> Vec<IndexSet<u32>> {
    let n = func.blocks.len();
    let mut live_out = vec![IndexSet::new(); n];
    let mut live_in = vec![IndexSet::new(); n];
    let mut changed = true;
    while changed {
        changed = false;
        for bi in (0..n).rev() {
            let mut out = IndexSet::new();
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
pub(crate) fn live_in_of(
    func: &MirFunction,
    live_out: &[IndexSet<u32>],
    bi: usize,
) -> IndexSet<u32> {
    let block = &func.blocks[bi];
    let mut inn = live_out[bi].clone();
    transfer_block(&block.stmts, &block.terminator, &mut inn);
    inn
}

/// Statement-level liveness of a function whose statements and terminators stay fixed while it is
/// queried. Each block records, per local, the statements that read or overwrite it, so
/// [`Liveness::after_stmt`] is a binary search instead of a rescan of the block tail; RC passes ask
/// it for every statement and local, which made long straight-line blocks cubic.
pub(crate) struct Liveness {
    out: Vec<IndexSet<u32>>,
    terminator_reads: Vec<IndexSet<u32>>,
    /// `(statement, reads)`: `reads` is false for a statement that overwrites the local without
    /// reading it.
    events: Vec<IndexMap<u32, Vec<(usize, bool)>>>,
}

impl Liveness {
    pub(crate) fn new(func: &MirFunction) -> Self {
        let out = live_out(func);
        let mut terminator_reads = Vec::with_capacity(func.blocks.len());
        let mut events = Vec::with_capacity(func.blocks.len());
        for block in &func.blocks {
            let mut reads = IndexSet::new();
            add_terminator_reads(&block.terminator, &mut reads);
            terminator_reads.push(reads);
            let mut block_events: IndexMap<u32, Vec<(usize, bool)>> = IndexMap::new();
            for (si, stmt) in block.stmts.iter().enumerate() {
                let reads = stmt_reads(stmt);
                for &local in &reads {
                    block_events.entry(local).or_default().push((si, true));
                }
                if let Statement::Assign(Place::Local(d), _) = stmt
                    && !reads.contains(&d.0)
                {
                    block_events.entry(d.0).or_default().push((si, false));
                }
            }
            events.push(block_events);
        }
        Self {
            out,
            terminator_reads,
            events,
        }
    }

    /// Per-block live-out sets, as returned by [`live_out`].
    pub(crate) fn out(&self) -> &[IndexSet<u32>] {
        &self.out
    }

    /// True if `local` may be read after statement `si` in block `bi` (not counting uses inside
    /// that statement itself).
    pub(crate) fn after_stmt(&self, bi: usize, si: usize, local: u32) -> bool {
        if let Some(events) = self.events[bi].get(&local) {
            let next = events.partition_point(|&(at, _)| at <= si);
            if let Some(&(_, reads)) = events.get(next) {
                return reads;
            }
        }
        self.terminator_reads[bi].contains(&local) || self.out[bi].contains(&local)
    }
}

fn transfer_block(stmts: &[Statement], term: &Terminator, live: &mut IndexSet<u32>) {
    if let Terminator::Await {
        dest: Some(local), ..
    } = term
    {
        // The resume edge defines a fresh result; later iterations do not read this generation.
        live.swap_remove(&local.0);
    }
    add_terminator_reads(term, live);
    for stmt in stmts.iter().rev() {
        transfer_stmt(stmt, live);
    }
}

pub(crate) fn transfer_stmt(stmt: &Statement, live: &mut IndexSet<u32>) {
    match stmt {
        Statement::RawStore { address, value, .. } => {
            add_operand_reads(address, live);
            add_operand_reads(value, live);
        }
        Statement::Assign(Place::Local(d), rv) => {
            live.swap_remove(&d.0);
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
        Statement::ArraySlotSwap {
            array,
            left,
            other_array,
            right,
            ..
        } => {
            add_operand_reads(array, live);
            add_operand_reads(other_array, live);
            add_operand_reads(left, live);
            add_operand_reads(right, live);
        }
        Statement::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => {
            add_operand_reads(dst, live);
            add_operand_reads(dst_off, live);
            add_operand_reads(count, live);
        }
        Statement::LockAcquire(o) | Statement::LockRelease(o) | Statement::DeferLeave(o) => {
            add_operand_reads(o, live)
        }
        Statement::DeferEnter | Statement::RegionEnter | Statement::RegionLeave => {}
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ..
        } => {
            add_operand_reads(dest, live);
            add_operand_reads(lhs, live);
            add_operand_reads(rhs, live);
            add_operand_reads(index, live);
            if let Some(s) = splat_rhs {
                add_operand_reads(s, live);
            }
        }
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
    }
}

/// True if `local` is read by `stmt` (plain operand, field/index base, or RC op).
pub(crate) fn stmt_reads_local(stmt: &Statement, local: u32) -> bool {
    stmt_reads(stmt).contains(&local)
}

/// Every local `stmt` reads (plain operand, field/index base, or RC op), in operand order.
pub(crate) fn stmt_reads(stmt: &Statement) -> IndexSet<u32> {
    let mut live = IndexSet::new();
    transfer_stmt_reads_only(stmt, &mut live);
    live
}

fn transfer_stmt_reads_only(stmt: &Statement, live: &mut IndexSet<u32>) {
    match stmt {
        Statement::RawStore { address, value, .. } => {
            add_operand_reads(address, live);
            add_operand_reads(value, live);
        }
        Statement::Assign(Place::Local(_), rv) => add_rvalue_reads(rv, live),
        Statement::Assign(place, rv) => {
            add_place_base_reads(place, live);
            add_rvalue_reads(rv, live);
        }
        _ => transfer_stmt(stmt, live),
    }
}

pub(crate) fn add_terminator_reads(term: &Terminator, live: &mut IndexSet<u32>) {
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

fn add_rvalue_reads(rv: &Rvalue, live: &mut IndexSet<u32>) {
    let mut add = |op: &Operand| add_operand_reads(op, live);
    match rv {
        Rvalue::Move { src, .. } => add(&Operand::Copy(Place::Local(*src))),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            add(cond);
            add(then_val);
            add(else_val);
        }
        Rvalue::RawLoad { address: o, .. }
        | Rvalue::ObservedLoad(o)
        | Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::CheckedNeg(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::StrBytes(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::TypeName(o)
        | Rvalue::Discriminant { base: o, .. }
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. } => add(o),
        Rvalue::Binary(_, a, b)
        | Rvalue::CheckedBinary(_, a, b)
        | Rvalue::CharAt(a, b, _)
        | Rvalue::ByteAt(a, b, _) => {
            add(a);
            add(b);
        }
        Rvalue::LoadU8(a, b, n) | Rvalue::LoadU16(a, b, n) => {
            add(a);
            add(b);
            if let Some(n) = n {
                add(n);
            }
        }
        Rvalue::Concat(parts) => {
            for p in parts {
                add(p);
            }
        }
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => {
            add(prefix);
            add(value);
            add(suffix);
        }
        Rvalue::EnumName { value, .. } => add(value),
        Rvalue::ArrayNew { len, .. } => add(len),
        Rvalue::ToBytes { value: o, .. } | Rvalue::FromBytes { bytes: o, .. } => add(o),
        Rvalue::ArraySlotTake { array, index, .. } => {
            add(array);
            add(index);
        }
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
        Rvalue::Zero(_) | Rvalue::FuncRef(_) => {}
    }
}

fn add_operand_reads(op: &Operand, live: &mut IndexSet<u32>) {
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

fn add_place_base_reads(place: &Place, live: &mut IndexSet<u32>) {
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

/// Interference graph for Future-frame slot sharing: `adj[a]` contains every
/// local that may be live at the same time as `a`. Two locals with disjoint
/// lifetimes can share one frame slot, since the frame is canonical storage —
/// scalars are re-read at poll entry and value locals alias it by pointer.
///
/// A definition interferes with everything live after it; an `await` result is
/// defined at the top of its resume block and therefore interferes with that
/// block's live-in set.
pub(crate) fn frame_interference(func: &MirFunction) -> Vec<BTreeSet<u32>> {
    let n = func.blocks.len();
    let mut live_out = vec![IndexSet::new(); n];
    let mut live_in = vec![IndexSet::new(); n];
    let mut changed = true;
    while changed {
        changed = false;
        for bi in (0..n).rev() {
            let mut out = IndexSet::new();
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
    let mut adj: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); func.locals.len()];
    let mut interfere = |a: u32, b: u32| {
        if a != b {
            adj[a as usize].insert(b);
            adj[b as usize].insert(a);
        }
    };
    for (block, block_live_out) in func.blocks.iter().zip(live_out.iter()) {
        // An await binds its result at the top of the resume block.
        if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
            for &v in block_live_out.iter() {
                interfere(v, d.0);
            }
        }
        let mut live = block_live_out.clone();
        add_terminator_reads(&block.terminator, &mut live);
        for stmt in block.stmts.iter().rev() {
            if let Statement::Assign(Place::Local(d), _) = stmt {
                for &v in live.iter() {
                    interfere(v, d.0);
                }
                live.swap_remove(&d.0);
            }
            transfer_stmt(stmt, &mut live);
        }
    }
    // Parameters are written by the constructor before the first poll, so they
    // all coexist at entry.
    for p in &func.params {
        for q in &func.params {
            if p.0 != q.0 {
                adj[p.0 as usize].insert(q.0);
            }
        }
    }
    adj
}
