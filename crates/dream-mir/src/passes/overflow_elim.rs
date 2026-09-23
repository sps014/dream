//! Overflow-check elimination: rewrites [`Rvalue::CheckedBinary`] / [`Rvalue::CheckedNeg`] into
//! their wrapping forms when the result provably fits its type, so counted loops and bounded
//! index arithmetic keep the plain arithmetic that induction-variable canonicalization,
//! vectorization, and LICM recognize.
//!
//! Operand ranges come from two sources:
//! - the single dominating definition of a local (constants, lengths, masks, remainders, narrowing
//!   casts, and arithmetic over those), and
//! - a dominating `x < y` / `x > y` branch edge for a local that is not redefined between that
//!   edge and the use — the loop-counter case, where the counter has several definitions.

use super::cfg::{predecessors, DomTree};
use super::MirPass;
use crate::int_ty::IntTy;
use crate::{
    BinOp, BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator, UnOp,
};
use dream_types::TypeInterner;

type Range = (i128, i128);

/// Bounds the recursive range derivation so long def chains stay cheap.
const MAX_DEPTH: u32 = 6;

pub struct OverflowElim;

impl MirPass for OverflowElim {
    fn name(&self) -> &'static str {
        "overflow-elim"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let has_checked = func.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::CheckedBinary(..) | Rvalue::CheckedNeg(_))
            )
        });
        if !has_checked {
            return false;
        }
        let cx = Cx::new(func, interner);
        let mut rewrites = Vec::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                let Statement::Assign(Place::Local(dest), rv) = stmt else {
                    continue;
                };
                let site = Site { block: bi, idx: si };
                if let Some(plain) = cx.unchecked_form(rv, *dest, site) {
                    rewrites.push((bi, si, plain));
                }
            }
        }
        let changed = !rewrites.is_empty();
        for (bi, si, plain) in rewrites {
            if let Statement::Assign(_, rv) = &mut func.blocks[bi].stmts[si] {
                *rv = plain;
            }
        }
        changed
    }
}

/// A program point: before statement `idx` of `block` (`idx == stmts.len()` is the terminator).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Site {
    block: usize,
    idx: usize,
}

/// A fact `lhs op rhs` that holds on entry to `target`, established by the branch at the end of
/// `guard`.
struct Guard<'f> {
    guard: usize,
    target: usize,
    op: BinOp,
    lhs: &'f Operand,
    rhs: &'f Operand,
    /// Where the comparison was computed, for ranging its operands.
    cmp_site: Site,
}

struct Cx<'f> {
    func: &'f MirFunction,
    interner: &'f TypeInterner,
    /// Every definition site of each local (`Await` destinations count as a def without a site).
    defs: Vec<Vec<Option<Site>>>,
    preds: Vec<Vec<BlockId>>,
    dom: DomTree,
    guards: Vec<Guard<'f>>,
}

impl<'f> Cx<'f> {
    fn new(func: &'f MirFunction, interner: &'f TypeInterner) -> Self {
        let mut defs = vec![Vec::new(); func.locals.len()];
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                if let Statement::Assign(Place::Local(d), _) = stmt {
                    defs[d.0 as usize].push(Some(Site { block: bi, idx: si }));
                }
            }
            if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
                defs[d.0 as usize].push(None);
            }
        }
        let preds = predecessors(func);
        let dom = DomTree::new(func);
        let mut cx = Cx {
            func,
            interner,
            defs,
            preds,
            dom,
            guards: Vec::new(),
        };
        cx.guards = cx.collect_guards();
        cx
    }

    fn collect_guards(&self) -> Vec<Guard<'f>> {
        let func = self.func;
        let mut guards = Vec::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            let Terminator::If {
                cond: Operand::Copy(Place::Local(c)),
                then_blk,
                else_blk,
            } = &block.terminator
            else {
                continue;
            };
            if then_blk == else_blk {
                continue;
            }
            let Some((si, Rvalue::Binary(op, lhs, rhs))) = last_def_in(func, bi, *c) else {
                continue;
            };
            let (Some(taken), Some(not_taken)) = (comparison(*op), comparison(*op).map(negate))
            else {
                continue;
            };
            let cmp_site = Site { block: bi, idx: si };
            for (target, op) in [(*then_blk, taken), (*else_blk, not_taken)] {
                if self.preds[target.0 as usize] == [BlockId(bi as u32)] {
                    guards.push(Guard {
                        guard: bi,
                        target: target.0 as usize,
                        op,
                        lhs,
                        rhs,
                        cmp_site,
                    });
                }
            }
        }
        guards
    }

    fn unchecked_form(&self, rv: &Rvalue, dest: Local, site: Site) -> Option<Rvalue> {
        let ty = IntTy::of(self.interner, self.func.local_ty(dest))?;
        match rv {
            Rvalue::CheckedBinary(op, a, b) => {
                let ra = self.operand_range(a, ty, site, 0);
                let rb = self.operand_range(b, ty, site, 0);
                let safe = match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul => {
                        arith(*op, ra, rb).is_some_and(|r| within(r, ty))
                    }
                    BinOp::Div | BinOp::Rem => !(contains(ra, ty.min()) && contains(rb, -1)),
                    BinOp::Shl | BinOp::Shr => rb.0 >= 0 && rb.1 < i128::from(ty.bits()),
                    _ => false,
                };
                safe.then(|| Rvalue::Binary(*op, a.clone(), b.clone()))
            }
            Rvalue::CheckedNeg(a) => {
                let r = self.operand_range(a, ty, site, 0);
                within((-r.1, -r.0), ty).then(|| Rvalue::Unary(UnOp::Neg, a.clone()))
            }
            _ => None,
        }
    }

    /// The range of `o` read at `site`, where a constant is interpreted at type `ty`.
    fn operand_range(&self, o: &Operand, ty: IntTy, site: Site, depth: u32) -> Range {
        match o {
            Operand::Const(Const::Int(v) | Const::Long(v)) => {
                let v = ty.value(*v);
                (v, v)
            }
            Operand::Const(Const::Char(c)) => {
                let v = i128::from(u32::from(*c));
                (v, v)
            }
            Operand::Copy(Place::Local(l)) => {
                match IntTy::of(self.interner, self.func.local_ty(*l)) {
                    Some(lty) => self.local_range(*l, lty, site, depth),
                    None => full(ty),
                }
            }
            _ => full(ty),
        }
    }

    fn local_range(&self, l: Local, ty: IntTy, site: Site, depth: u32) -> Range {
        if depth >= MAX_DEPTH {
            return full(ty);
        }
        let defs = &self.defs[l.0 as usize];
        let mut range = match defs.as_slice() {
            [Some(def)] if self.dominates(*def, site) => {
                let Statement::Assign(_, rv) = &self.func.blocks[def.block].stmts[def.idx] else {
                    unreachable!("def sites are assignments")
                };
                self.rvalue_range(rv, ty, *def, depth + 1)
            }
            _ => full(ty),
        };
        for g in &self.guards {
            if let Some(bound) = self.guard_bound(g, l, ty, site, depth + 1) {
                range = intersect(range, bound);
            }
        }
        range
    }

    /// A bound on `l` at `site` implied by `g`, if `g` constrains `l` and still holds there.
    fn guard_bound(&self, g: &Guard, l: Local, ty: IntTy, site: Site, depth: u32) -> Option<Range> {
        let is_l = |o: &Operand| matches!(o, Operand::Copy(Place::Local(x)) if *x == l);
        let (op, other) = if is_l(g.lhs) {
            (g.op, g.rhs)
        } else if is_l(g.rhs) {
            (flip(g.op), g.lhs)
        } else {
            return None;
        };
        if !self.guard_holds(g, l, site) {
            return None;
        }
        // Guard chains fan out per guard, so they get a shorter budget than def chains.
        let depth = depth.max(MAX_DEPTH - 2);
        let (lo, hi) = self.operand_range(other, ty, g.cmp_site, depth);
        Some(match op {
            BinOp::Lt => (ty.min(), hi - 1),
            BinOp::Le => (ty.min(), hi),
            BinOp::Gt => (lo + 1, ty.max()),
            BinOp::Ge => (lo, ty.max()),
            _ => return None,
        })
    }

    /// True when `l` keeps the value it had at `g`'s comparison on every path from that
    /// comparison to `site`, and every such path enters through `g.target`.
    fn guard_holds(&self, g: &Guard, l: Local, site: Site) -> bool {
        let assigns = |block: usize, stmts: std::ops::Range<usize>| {
            self.func.blocks[block].stmts[stmts]
                .iter()
                .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == l))
        };
        let redefines_block = |block: usize| {
            assigns(block, 0..self.func.blocks[block].stmts.len())
                || matches!(
                    self.func.blocks[block].terminator,
                    Terminator::Await { dest: Some(d), .. } if d == l
                )
        };
        let guard_len = self.func.blocks[g.guard].stmts.len();
        if assigns(g.guard, g.cmp_site.idx + 1..guard_len)
            || !self
                .dom
                .dominates(BlockId(g.target as u32), BlockId(site.block as u32))
            || assigns(site.block, 0..site.idx)
        {
            return false;
        }
        if site.block == g.target {
            return true;
        }
        let mut seen = vec![false; self.func.blocks.len()];
        let mut stack: Vec<usize> = self.preds[site.block]
            .iter()
            .map(|b| b.0 as usize)
            .collect();
        while let Some(b) = stack.pop() {
            if std::mem::replace(&mut seen[b], true) {
                continue;
            }
            if b == g.guard || redefines_block(b) {
                return false;
            }
            if b != g.target {
                stack.extend(self.preds[b].iter().map(|p| p.0 as usize));
            }
        }
        true
    }

    fn rvalue_range(&self, rv: &Rvalue, ty: IntTy, site: Site, depth: u32) -> Range {
        let operand = |o: &Operand| self.operand_range(o, ty, site, depth);
        let derived = match rv {
            Rvalue::Use(o) => Some(operand(o)),
            Rvalue::ArrayLen(_) | Rvalue::StrLen(_) | Rvalue::StrByteSize(_) => {
                Some((0, i128::from(i32::MAX)))
            }
            Rvalue::Cast(o, from, _) => match IntTy::of(self.interner, *from) {
                Some(from) => {
                    Some(self.operand_range(o, from, site, depth)).filter(|r| within(*r, ty))
                }
                None => None,
            },
            Rvalue::Binary(op, a, b) => bitwise_range(*op, operand(a), operand(b), ty)
                .or_else(|| arith(*op, operand(a), operand(b)).filter(|r| within(*r, ty))),
            Rvalue::CheckedBinary(op, a, b) => bitwise_range(*op, operand(a), operand(b), ty)
                .or_else(|| arith(*op, operand(a), operand(b)).map(|r| intersect(r, full(ty)))),
            Rvalue::Unary(UnOp::Neg, a) => {
                let r = operand(a);
                Some((-r.1, -r.0)).filter(|r| within(*r, ty))
            }
            Rvalue::CheckedNeg(a) => {
                let r = operand(a);
                Some(intersect((-r.1, -r.0), full(ty)))
            }
            Rvalue::Select {
                then_val, else_val, ..
            } => {
                let (t, e) = (operand(then_val), operand(else_val));
                Some((t.0.min(e.0), t.1.max(e.1)))
            }
            _ => None,
        };
        derived.unwrap_or_else(|| full(ty))
    }

    fn dominates(&self, def: Site, use_site: Site) -> bool {
        if def.block == use_site.block {
            def.idx < use_site.idx
        } else {
            self.dom
                .dominates(BlockId(def.block as u32), BlockId(use_site.block as u32))
        }
    }
}

/// The exact range of `a op b` over the operand ranges, for the arithmetic ops.
fn arith(op: BinOp, a: Range, b: Range) -> Option<Range> {
    match op {
        BinOp::Add => Some((a.0.checked_add(b.0)?, a.1.checked_add(b.1)?)),
        BinOp::Sub => Some((a.0.checked_sub(b.1)?, a.1.checked_sub(b.0)?)),
        BinOp::Mul => {
            let products = [
                a.0.checked_mul(b.0)?,
                a.0.checked_mul(b.1)?,
                a.1.checked_mul(b.0)?,
                a.1.checked_mul(b.1)?,
            ];
            Some((*products.iter().min()?, *products.iter().max()?))
        }
        _ => None,
    }
}

/// Ranges for ops whose result is bounded by one operand regardless of wrapping.
fn bitwise_range(op: BinOp, a: Range, b: Range, ty: IntTy) -> Option<Range> {
    match op {
        BinOp::BitAnd if a.0 >= 0 && b.0 >= 0 => Some((0, a.1.min(b.1))),
        BinOp::BitAnd if a.0 >= 0 => Some((0, a.1)),
        BinOp::BitAnd if b.0 >= 0 => Some((0, b.1)),
        BinOp::Rem if b.0 == b.1 && b.0 > 0 => {
            let m = b.0 - 1;
            Some(if a.0 >= 0 { (0, a.1.min(m)) } else { (-m, m) })
        }
        BinOp::Div if b.0 == b.1 && b.0 > 0 => Some((a.0 / b.0, a.1 / b.0)),
        BinOp::Shr if b.0 == b.1 && b.0 >= 0 && b.0 < i128::from(ty.bits()) => {
            Some((a.0 >> b.0, a.1 >> b.0))
        }
        _ => None,
    }
}

fn comparison(op: BinOp) -> Option<BinOp> {
    matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge).then_some(op)
}

fn negate(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Ge,
        BinOp::Le => BinOp::Gt,
        BinOp::Gt => BinOp::Le,
        _ => BinOp::Lt,
    }
}

/// The same fact with the operands swapped (`a < b` ⇔ `b > a`).
fn flip(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Gt,
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        BinOp::Ge => BinOp::Le,
        other => other,
    }
}

fn last_def_in(func: &MirFunction, block: usize, l: Local) -> Option<(usize, &Rvalue)> {
    func.blocks[block]
        .stmts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, s)| match s {
            Statement::Assign(Place::Local(d), rv) if *d == l => Some((i, rv)),
            _ => None,
        })
}

fn full(ty: IntTy) -> Range {
    (ty.min(), ty.max())
}

fn within(r: Range, ty: IntTy) -> bool {
    ty.fits(r.0) && ty.fits(r.1)
}

fn contains(r: Range, v: i128) -> bool {
    r.0 <= v && v <= r.1
}

fn intersect(a: Range, b: Range) -> Range {
    (a.0.max(b.0), a.1.min(b.1))
}

#[cfg(test)]
#[path = "overflow_elim_tests.rs"]
mod tests;
