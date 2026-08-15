//! Common-subexpression elimination for pure integer/boolean arithmetic. A `Binary`/`Unary`
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
use crate::{BinOp, BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, UnOp};
use dream_types::TypeInterner;

pub struct Gvn;

/// A canonical, hashable key for a redundancy-eligible expression.
#[derive(PartialEq, Eq, Hash, Clone)]
enum Key {
    Binary(BinOp, OpKey, OpKey),
    Unary(UnOp, OpKey),
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum OpKey {
    Local(u32),
    Int(i64),
    Long(i64),
    Bool(bool),
    Char(char),
}

impl MirPass for Gvn {
    fn name(&self) -> &'static str {
        "gvn"
    }

    fn run(&self, func: &mut MirFunction, _interner: &TypeInterner) -> bool {
        let n = func.blocks.len();
        if n == 0 {
            return false;
        }
        let rpo = super::cfg::reverse_postorder(func);
        let preds = super::cfg::predecessors(func);
        let mut exit_avail: Vec<Option<Vec<(Key, u32)>>> = vec![None; n];
        let mut entry_avail: Vec<Vec<(Key, u32)>> = vec![Vec::new(); n];

        let mut df_changed = true;
        while df_changed {
            df_changed = false;
            for &bid in &rpo {
                let bi = bid.0 as usize;
                let avail = meet_avail(&preds[bi], &exit_avail);
                entry_avail[bi].clone_from(&avail);
                let exit = transfer_avail(&func.blocks[bi].stmts, avail);
                if exit_avail[bi].as_ref() != Some(&exit) {
                    exit_avail[bi] = Some(exit);
                    df_changed = true;
                }
            }
        }

        let mut changed = false;
        for &bid in &rpo {
            let mut avail = entry_avail[bid.0 as usize].clone();
            for stmt in &mut func.blocks[bid.0 as usize].stmts {
                changed |= rewrite_cse(stmt, &mut avail);
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

fn transfer_avail(stmts: &[Statement], mut avail: Vec<(Key, u32)>) -> Vec<(Key, u32)> {
    for stmt in stmts {
        apply_avail(stmt, &mut avail);
    }
    avail
}

fn rewrite_cse(stmt: &mut Statement, avail: &mut Vec<(Key, u32)>) -> bool {
    let mut changed = false;
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        let dest_id = dest.0;
        let key = key_of(rvalue);
        if let Some(ref k) = key {
            if let Some(&(_, src)) = avail.iter().find(|(ak, l)| ak == k && *l != dest_id) {
                *rvalue = Rvalue::Use(Operand::Copy(Place::Local(Local(src))));
                changed = true;
            }
        }
        invalidate(avail, dest_id);
        if let Some(k) = key {
            avail.push((k, dest_id));
        }
    }
    changed
}

fn apply_avail(stmt: &Statement, avail: &mut Vec<(Key, u32)>) {
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        let dest_id = dest.0;
        let key = key_of(rvalue);
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
        Key::Binary(_, a, b) => mentions(a) || mentions(b),
        Key::Unary(_, a) => mentions(a),
    }
}

fn key_of(rvalue: &Rvalue) -> Option<Key> {
    match rvalue {
        Rvalue::Binary(op, a, b) => Some(Key::Binary(*op, op_key(a)?, op_key(b)?)),
        Rvalue::Unary(op, a) => Some(Key::Unary(*op, op_key(a)?)),
        _ => None,
    }
}

/// A hashable key for an operand, or `None` for shapes we refuse to number (field/index/global
/// reads, floats, strings, null). Returning `None` disables CSE for that expression: a shared
/// `Opaque` key would make every `s == "int"` match every `s == "string"` (and every `a.x + 1`
/// match `a.y + 1`), which is how `@json` treated `string` fields as unsupported.
fn op_key(op: &Operand) -> Option<OpKey> {
    Some(match op {
        Operand::Copy(Place::Local(l)) => OpKey::Local(l.0),
        Operand::Copy(_) => return None,
        Operand::Const(Const::Int(v)) => OpKey::Int(*v),
        Operand::Const(Const::Long(v)) => OpKey::Long(*v),
        Operand::Const(Const::Bool(b)) => OpKey::Bool(*b),
        Operand::Const(Const::Char(c)) => OpKey::Char(*c),
        Operand::Const(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Operand, Place, Rvalue, Terminator};

    #[test]
    fn dedups_repeated_binary() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let x = b.new_param(i.int(), Some("x".into()));
        let a = b.new_temp(i.int());
        let c = b.new_temp(i.int());
        let mul = || {
            Rvalue::Binary(
                BinOp::Mul,
                Operand::Copy(Place::Local(x)),
                Operand::Const(Const::Int(4)),
            )
        };
        b.assign(Place::Local(a), mul());
        b.assign(Place::Local(c), mul());
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
        let mut func = b.finish();
        assert!(Gvn.run(&mut func, &i));
        // Second `x*4` becomes `Use(a)`.
        match &func.blocks[0].stmts[1] {
            Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Local(l)))) => assert_eq!(*l, a),
            other => panic!("expected CSE copy, got {:?}", other),
        }
    }

    #[test]
    fn invalidates_on_operand_redef() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let x = b.new_local(i.int(), Some("x".into()));
        let a = b.new_temp(i.int());
        let c = b.new_temp(i.int());
        b.assign(
            Place::Local(a),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(x)),
                Operand::Const(Const::Int(1)),
            ),
        );
        // Redefine x between the two adds: the second must NOT be CSE'd.
        b.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Int(9))));
        b.assign(
            Place::Local(c),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(x)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
        let mut func = b.finish();
        Gvn.run(&mut func, &i);
        assert!(matches!(
            &func.blocks[0].stmts[2],
            Statement::Assign(_, Rvalue::Binary(..))
        ));
    }

    /// Two loops in one function (inlined `foreach` after IV): the first loop's bump pointer is
    /// initialized to `x+4` then mutated on the latch. A later `x+4` must not CSE to that pointer.
    #[test]
    fn does_not_cse_add_across_loop_back_edge() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let x = b.new_param(i.int(), Some("x".into()));
        let c = b.new_param(i.bool(), Some("c".into()));
        let p = b.new_temp(i.int());
        let q = b.new_temp(i.int());
        let header = b.new_block();
        let latch = b.new_block();
        let after = b.new_block();
        let add4 = || {
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(x)),
                Operand::Const(Const::Int(4)),
            )
        };
        b.assign(Place::Local(p), add4());
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(c)),
            then_blk: latch,
            else_blk: after,
        });
        b.switch_to(latch);
        b.assign(
            Place::Local(p),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(p)),
                Operand::Const(Const::Int(4)),
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(after);
        b.assign(Place::Local(q), add4());
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(q)))));
        let mut func = b.finish();
        Gvn.run(&mut func, &i);
        match &func.blocks[after.0 as usize].stmts[0] {
            Statement::Assign(_, Rvalue::Binary(..)) => {}
            other => panic!(
                "post-loop x+4 must not CSE to the loop bump pointer, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn does_not_cse_string_eq_with_different_literals() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.bool());
        let s = b.new_param(i.string(), Some("s".into()));
        let a = b.new_temp(i.bool());
        let c = b.new_temp(i.bool());
        b.assign(
            Place::Local(a),
            Rvalue::Binary(
                BinOp::Eq,
                Operand::Copy(Place::Local(s)),
                Operand::Const(Const::Str("int".into())),
            ),
        );
        b.assign(
            Place::Local(c),
            Rvalue::Binary(
                BinOp::Eq,
                Operand::Copy(Place::Local(s)),
                Operand::Const(Const::Str("string".into())),
            ),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(c)))));
        let mut func = b.finish();
        Gvn.run(&mut func, &i);
        match &func.blocks[0].stmts[1] {
            Statement::Assign(_, Rvalue::Binary(BinOp::Eq, _, _)) => {}
            other => panic!(
                "s == \"string\" must not CSE to s == \"int\", got {:?}",
                other
            ),
        }
    }
}
