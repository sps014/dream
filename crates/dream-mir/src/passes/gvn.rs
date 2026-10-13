//! Common-subexpression elimination for pure integer/boolean/float arithmetic. A `Binary`/`Unary`
//! computation with the same operator and operands as an available earlier result is replaced by a
//! copy of that result. Copy/constant propagation and DCE then clean up the redundant copy.
//!
//! Restricted to `Binary`/`Unary` over constants and local reads: these never touch memory, so no
//! call or store between the two occurrences can invalidate the value. The only way a cached
//! expression becomes stale is a reassignment of one of its operand locals (or its own result
//! local), which is handled by invalidation.
//!
//! Availability is a forward dataflow to a fixpoint. Loop headers meet the preheader *and* the
//! latch; a single RPO pass would ignore the not-yet-visited back edge and treat a bump pointer as
//! still equal to `arr + 4` in a later loop (see `does_not_cse_add_across_loop_back_edge`).

use super::MirPass;
use crate::{
    BinOp, BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator, UnOp,
};
use dream_types::TypeInterner;

pub struct Gvn;

/// A canonical, hashable key for a redundancy-eligible expression.
#[derive(PartialEq, Eq, Hash, Clone)]
enum Key {
    Binary(BinOp, OpKey, OpKey),
    Unary(UnOp, OpKey),
    /// Kept apart from `Binary`: a wrapped result must not stand in for a checked op, which has to
    /// panic where the wrapped one silently wrapped.
    Checked(BinOp, OpKey, OpKey),
    CheckedNeg(OpKey),
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum OpKey {
    Local(u32),
    Int(i64),
    Long(i64),
    Bool(bool),
    Char(char),
    /// Floats are keyed on bits, not `PartialEq`: `-0.0 != 0.0` bitwise but compares equal,
    /// and `x / 0.0` vs `x / -0.0` differ (+inf vs -inf), so they must not share a key.
    F64(u64),
    F32(u32),
}

impl MirPass for Gvn {
    fn preserves(&self) -> crate::passes::PreservedAnalyses {
        crate::passes::PreservedAnalyses::ControlFlow
    }

    fn name(&self) -> &'static str {
        "gvn"
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        _layouts: &dream_hir::LayoutTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
        let n = func.blocks.len();
        if n == 0 {
            return false;
        }
        let scalar = super::scalar_operands::local_flags(func, interner);
        let rpo = analyses.reverse_postorder(func);
        let preds = analyses.predecessors(func);
        let mut exit_avail: Vec<Option<Vec<(Key, u32)>>> = vec![None; n];
        let mut entry_avail: Vec<Vec<(Key, u32)>> = vec![Vec::new(); n];

        let mut df_changed = true;
        while df_changed {
            df_changed = false;
            for &bid in rpo.iter() {
                let bi = bid.0 as usize;
                let avail = meet_avail(&preds[bi], &exit_avail);
                entry_avail[bi].clone_from(&avail);
                let mut exit = transfer_avail(&func.blocks[bi].stmts, avail, &scalar);
                if let Terminator::Await {
                    dest: Some(dest), ..
                } = func.blocks[bi].terminator
                {
                    invalidate(&mut exit, dest.0);
                }
                if exit_avail[bi].as_ref() != Some(&exit) {
                    exit_avail[bi] = Some(exit);
                    df_changed = true;
                }
            }
        }

        let mut changed = false;
        for &bid in rpo.iter() {
            let mut avail = entry_avail[bid.0 as usize].clone();
            for stmt in &mut func.blocks[bid.0 as usize].stmts {
                changed |= rewrite_cse(stmt, &mut avail, &scalar);
            }
        }
        changed
    }
}

/// Intersection of predecessor exit sets. A not-yet-computed predecessor (back edge on the first
/// visit) contributes no facts — using the visited preds alone would treat loop-carried bump
/// pointers as still equal to their preheader init.
fn meet_avail(preds: &[BlockId], exit: &[Option<Vec<(Key, u32)>>]) -> Vec<(Key, u32)> {
    if preds.is_empty() {
        return Vec::new();
    }
    let mut states: Vec<&Vec<(Key, u32)>> = Vec::with_capacity(preds.len());
    for p in preds {
        match &exit[p.0 as usize] {
            Some(s) => states.push(s),
            None => return Vec::new(),
        }
    }
    let mut acc = states[0].clone();
    for other in &states[1..] {
        acc.retain(|(k, l)| other.iter().any(|(ok, ol)| ok == k && ol == l));
    }
    acc
}

fn transfer_avail(
    stmts: &[Statement],
    mut avail: Vec<(Key, u32)>,
    scalar: &[bool],
) -> Vec<(Key, u32)> {
    for stmt in stmts {
        apply_avail(stmt, &mut avail, scalar);
    }
    avail
}

fn rewrite_cse(stmt: &mut Statement, avail: &mut Vec<(Key, u32)>, scalar: &[bool]) -> bool {
    let mut changed = false;
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        let dest_id = dest.0;
        let key = key_of(rvalue, scalar);
        if let Some(ref k) = key
            && let Some(&(_, src)) = avail.iter().find(|(ak, l)| ak == k && *l != dest_id)
        {
            *rvalue = Rvalue::Use(Operand::Copy(Place::Local(Local(src))));
            changed = true;
        }
        invalidate(avail, dest_id);
        if let Some(k) = key {
            avail.push((k, dest_id));
        }
    }
    changed
}

fn apply_avail(stmt: &Statement, avail: &mut Vec<(Key, u32)>, scalar: &[bool]) {
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        let dest_id = dest.0;
        let key = key_of(rvalue, scalar);
        invalidate(avail, dest_id);
        if let Some(k) = key {
            avail.push((k, dest_id));
        }
    }
}

/// Drops every available entry defined into `dest` or reading `dest` as an operand (its value is now
/// stale).
fn invalidate(avail: &mut Vec<(Key, u32)>, dest: u32) {
    avail.retain(|(k, l)| *l != dest && !key_mentions(k, dest));
}

fn key_mentions(k: &Key, local: u32) -> bool {
    let mentions = |o: &OpKey| matches!(o, OpKey::Local(l) if *l == local);
    match k {
        Key::Binary(_, a, b) | Key::Checked(_, a, b) => mentions(a) || mentions(b),
        Key::Unary(_, a) | Key::CheckedNeg(a) => mentions(a),
    }
}

fn key_of(rvalue: &Rvalue, scalar: &[bool]) -> Option<Key> {
    match rvalue {
        Rvalue::Binary(op, a, b) => Some(Key::Binary(*op, op_key(a, scalar)?, op_key(b, scalar)?)),
        Rvalue::Unary(op, a) => Some(Key::Unary(*op, op_key(a, scalar)?)),
        Rvalue::CheckedBinary(op, a, b) => {
            Some(Key::Checked(*op, op_key(a, scalar)?, op_key(b, scalar)?))
        }
        Rvalue::CheckedNeg(a) => Some(Key::CheckedNeg(op_key(a, scalar)?)),
        _ => None,
    }
}

/// A hashable key for an operand, or `None` for shapes we refuse to number (field/index/global
/// reads, non-scalar locals, strings, null). Returning `None` disables CSE for that expression: a
/// shared `Opaque` key would make every `s == "int"` match every `s == "string"` (and every
/// `a.x + 1` match `a.y + 1`), which is how `@json` treated `string` fields as unsupported.
fn op_key(op: &Operand, scalar: &[bool]) -> Option<OpKey> {
    Some(match op {
        Operand::Copy(Place::Local(l)) if scalar[l.0 as usize] => OpKey::Local(l.0),
        Operand::Copy(_) => return None,
        Operand::Const(Const::Int(v)) => OpKey::Int(*v),
        Operand::Const(Const::Long(v)) => OpKey::Long(*v),
        Operand::Const(Const::Bool(b)) => OpKey::Bool(*b),
        Operand::Const(Const::Char(c)) => OpKey::Char(*c),
        // Bit-keyed (see `OpKey::F64`); identical bits give identical results for every
        // IEEE op we CSE, NaN payloads included.
        Operand::Const(Const::Float(v)) => OpKey::F64(v.to_bits()),
        Operand::Const(Const::F32(v)) => OpKey::F32(v.to_bits()),
        Operand::Const(_) => return None,
    })
}

#[cfg(test)]
#[path = "gvn_tests.rs"]
mod tests;
