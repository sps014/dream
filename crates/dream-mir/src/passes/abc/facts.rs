//! Range facts for bounds-check elimination.
//!
//! Guard facts hold on one outgoing edge of an `If` whose condition compares a local. They are
//! propagated to every block the edge's target dominates, minus blocks a redefinition of one of
//! the fact's locals can reach without re-crossing the guard; inside a block, a redefinition kills
//! the fact from that statement on. Global facts hold at every use because every definition of
//! the local preserves them.

use super::special;
use super::{as_local, const_int, str_base};
use crate::passes::cfg::{predecessors, DomTree};
use crate::{BinOp, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum StrBase {
    Local(u32),
    Lit(String),
}

/// A length source: an array local, or a string's code-unit / UTF-8 byte length.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Bound {
    Arr(u32),
    Unit(StrBase),
    Byte(StrBase),
}

impl Bound {
    fn local(&self) -> Option<u32> {
        match self {
            Bound::Arr(a) => Some(*a),
            Bound::Unit(StrBase::Local(s)) | Bound::Byte(StrBase::Local(s)) => Some(*s),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Fact {
    /// `idx < len(bound)`.
    Below(u32, Bound),
    /// `idx < k`.
    BelowConst(u32, i64),
    /// `idx >= 0`.
    NonNeg(u32),
    /// `idx < v` for some `int` value `v`, so `idx + 1` cannot wrap.
    Bounded(u32),
}

impl Fact {
    fn mentions(&self, l: u32) -> bool {
        match self {
            Fact::Below(i, b) => *i == l || b.local() == Some(l),
            Fact::BelowConst(i, _) | Fact::NonNeg(i) | Fact::Bounded(i) => *i == l,
        }
    }

    fn locals(&self) -> Vec<u32> {
        match self {
            Fact::Below(i, b) => {
                let mut v = vec![*i];
                v.extend(b.local());
                v
            }
            Fact::BelowConst(i, _) | Fact::NonNeg(i) | Fact::Bounded(i) => vec![*i],
        }
    }
}

/// Definition sites per local: every `Assign(Place::Local(_), _)` plus `Await` result bindings.
pub(super) struct Defs {
    count: Vec<u32>,
    /// Position of the sole `Assign` when the local has exactly one definition and it is one.
    sole: Vec<Option<(usize, usize)>>,
    blocks: Vec<Vec<usize>>,
    params: BTreeSet<u32>,
}

impl Defs {
    pub(super) fn new(func: &MirFunction) -> Defs {
        let n = func.locals.len();
        let mut count = vec![0u32; n];
        let mut sole = vec![None; n];
        let mut blocks: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut note = |l: usize, bi: usize, pos: Option<usize>, count: &mut Vec<u32>| {
            if l >= n {
                return;
            }
            count[l] += 1;
            sole[l] = if count[l] == 1 {
                pos.map(|si| (bi, si))
            } else {
                None
            };
            if blocks[l].last() != Some(&bi) {
                blocks[l].push(bi);
            }
        };
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                if let Statement::Assign(Place::Local(d), _) = stmt {
                    note(d.0 as usize, bi, Some(si), &mut count);
                }
            }
            if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
                note(d.0 as usize, bi, None, &mut count);
            }
        }
        Defs {
            count,
            sole,
            blocks,
            params: func.params.iter().map(|p| p.0).collect(),
        }
    }

    pub(super) fn count(&self, l: u32) -> u32 {
        self.count.get(l as usize).copied().unwrap_or(0)
    }

    pub(super) fn blocks(&self, l: u32) -> &[usize] {
        self.blocks.get(l as usize).map_or(&[], |v| v.as_slice())
    }

    /// The local never changes once it holds a value: an unassigned parameter, or one definition.
    pub(super) fn stable(&self, l: u32) -> bool {
        match self.count(l) {
            0 => self.params.contains(&l),
            1 => self.sole.get(l as usize).is_some_and(|s| s.is_some()),
            _ => false,
        }
    }

    pub(super) fn sole_def<'f>(
        &self,
        func: &'f MirFunction,
        l: u32,
    ) -> Option<(&'f Rvalue, (usize, usize))> {
        let (bi, si) = (*self.sole.get(l as usize)?)?;
        match &func.blocks[bi].stmts[si] {
            Statement::Assign(_, rv) => Some((rv, (bi, si))),
            _ => None,
        }
    }

    pub(super) fn const_value(&self, func: &MirFunction, op: &Operand) -> Option<i64> {
        match op {
            Operand::Copy(Place::Local(l)) => match self.sole_def(func, l.0)? {
                (Rvalue::Use(Operand::Const(crate::Const::Int(v))), _) => Some(*v),
                _ => None,
            },
            _ => const_int(op),
        }
    }

    pub(super) fn is_param(&self, l: u32) -> bool {
        self.params.contains(&l)
    }
}

/// Facts that hold wherever the local is read.
#[derive(Default)]
pub(super) struct Globals {
    pub(super) nonneg: BTreeSet<u32>,
    pub(super) below: BTreeSet<(u32, Bound)>,
}

/// Facts at one program point: guard facts still alive here plus the global facts.
pub(super) struct FactView<'g> {
    local: BTreeSet<Fact>,
    globals: &'g Globals,
}

impl FactView<'_> {
    pub(super) fn holds(&self, f: &Fact) -> bool {
        if self.local.contains(f) {
            return true;
        }
        match f {
            Fact::Below(i, b) => self.globals.below.contains(&(*i, b.clone())),
            Fact::NonNeg(i) => self.globals.nonneg.contains(i),
            _ => false,
        }
    }

    pub(super) fn nonneg(&self, i: u32) -> bool {
        self.holds(&Fact::NonNeg(i))
    }

    /// Drops every guard fact that mentions a local `stmt` (re)defines.
    pub(super) fn kill_defs(&mut self, stmt: &Statement) {
        if let Statement::Assign(Place::Local(d), _) = stmt {
            self.local.retain(|f| !f.mentions(d.0));
        }
    }
}

pub(super) struct FactEngine {
    entry: Vec<BTreeSet<Fact>>,
    globals: Globals,
}

impl FactEngine {
    pub(super) fn new(func: &MirFunction) -> FactEngine {
        let defs = Defs::new(func);
        let entry = guard_entry_facts(func, &defs);
        let empty = Globals::default();
        let mut bounded_incr = BTreeSet::new();
        let mut nonneg_decr = BTreeSet::new();
        scan(func, &entry, &empty, |bi, si, stmt, view| {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                return;
            };
            if let Some(x) = unit_increment_of(rv) {
                if view.holds(&Fact::Bounded(x.0)) {
                    bounded_incr.insert((bi, si));
                }
            }
            if decrement_of(rv) == Some(*d) && view.nonneg(d.0) {
                nonneg_decr.insert((bi, si));
            }
        });
        let mut globals = Globals {
            nonneg: global_nonneg(func, &defs, &bounded_incr),
            below: BTreeSet::new(),
        };
        globals.below = decreasing_below(func, &defs, &nonneg_decr);
        let affine = special::affine_facts(func, &defs, &entry, &globals);
        for (idx, bounds) in affine {
            globals.nonneg.insert(idx);
            for b in bounds {
                globals.below.insert((idx, b));
            }
        }
        FactEngine { entry, globals }
    }

    pub(super) fn entry_view(&self, bi: usize) -> FactView<'_> {
        FactView {
            local: self.entry[bi].clone(),
            globals: &self.globals,
        }
    }
}

/// Visits every statement with the facts that hold just before it.
pub(super) fn scan(
    func: &MirFunction,
    entry: &[BTreeSet<Fact>],
    globals: &Globals,
    mut f: impl FnMut(usize, usize, &Statement, &FactView<'_>),
) {
    for (bi, block) in func.blocks.iter().enumerate() {
        let mut view = FactView {
            local: entry[bi].clone(),
            globals,
        };
        for (si, stmt) in block.stmts.iter().enumerate() {
            f(bi, si, stmt, &view);
            view.kill_defs(stmt);
        }
    }
}

fn guard_entry_facts(func: &MirFunction, defs: &Defs) -> Vec<BTreeSet<Fact>> {
    let n = func.blocks.len();
    let mut entry = vec![BTreeSet::new(); n];
    let preds = predecessors(func);
    let mut children: Option<Vec<Vec<usize>>> = None;
    for (gi, block) in func.blocks.iter().enumerate() {
        let Terminator::If {
            cond: Operand::Copy(Place::Local(c)),
            then_blk,
            else_blk,
        } = &block.terminator
        else {
            continue;
        };
        let (then_facts, else_facts) = guard_facts(func, defs, gi, *c);
        for (succ, facts) in [(then_blk.0 as usize, then_facts), (else_blk.0 as usize, else_facts)] {
            if facts.is_empty() || succ == gi || preds[succ].len() != 1 {
                continue;
            }
            let children = children.get_or_insert_with(|| dom_children(func, &DomTree::new(func)));
            propagate(func, defs, children, succ, facts, &mut entry);
        }
    }
    entry
}

fn dom_children(func: &MirFunction, dom: &DomTree) -> Vec<Vec<usize>> {
    let n = func.blocks.len();
    let mut children = vec![Vec::new(); n];
    for b in 0..n {
        if let Some(p) = dom.idom(crate::BlockId(b as u32)) {
            if p.0 as usize != b {
                children[p.0 as usize].push(b);
            }
        }
    }
    children
}

/// Adds `facts` to the entry of every block `succ` dominates that no redefinition of a fact local
/// can reach without passing back through `succ` (whose only way in is the guard edge).
fn propagate(
    func: &MirFunction,
    defs: &Defs,
    children: &[Vec<usize>],
    succ: usize,
    facts: Vec<Fact>,
    entry: &mut [BTreeSet<Fact>],
) {
    let n = func.blocks.len();
    let region = reach_avoiding(func, &[succ], succ, n);
    let mut tainted: BTreeMap<u32, Vec<bool>> = BTreeMap::new();
    for fact in &facts {
        for l in fact.locals() {
            tainted.entry(l).or_insert_with(|| {
                let starts: Vec<usize> = defs
                    .blocks(l)
                    .iter()
                    .copied()
                    .filter(|&d| d == succ || region[d])
                    .collect();
                reach_avoiding(func, &starts, succ, n)
            });
        }
    }
    let mut stack = vec![succ];
    while let Some(t) = stack.pop() {
        for fact in &facts {
            if fact.locals().iter().all(|l| !tainted[l][t]) {
                entry[t].insert(fact.clone());
            }
        }
        stack.extend(children[t].iter().copied());
    }
}

/// Blocks reachable through at least one edge from `starts`, never entering `avoid`.
fn reach_avoiding(func: &MirFunction, starts: &[usize], avoid: usize, n: usize) -> Vec<bool> {
    let mut seen = vec![false; n];
    let mut stack: Vec<usize> = starts.to_vec();
    while let Some(b) = stack.pop() {
        for s in func.blocks[b].terminator.successors() {
            let s = s.0 as usize;
            if s != avoid && !seen[s] {
                seen[s] = true;
                stack.push(s);
            }
        }
    }
    seen
}

/// Facts that hold on the then- and else-edges of block `gi`'s `If` on `cmp`.
fn guard_facts(func: &MirFunction, defs: &Defs, gi: usize, cmp: Local) -> (Vec<Fact>, Vec<Fact>) {
    let stmts = &func.blocks[gi].stmts;
    let Some(k) = stmts
        .iter()
        .rposition(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == cmp))
    else {
        return (Vec::new(), Vec::new());
    };
    let Statement::Assign(_, Rvalue::Binary(op, x, y)) = &stmts[k] else {
        return (Vec::new(), Vec::new());
    };
    let at = Some((gi, k));
    let (lt, x, y) = match op {
        BinOp::Lt => (true, x, y),
        BinOp::Gt => (true, y, x),
        BinOp::Le => (false, x, y),
        BinOp::Ge => (false, y, x),
        _ => return (Vec::new(), Vec::new()),
    };
    let mut then_f = Vec::new();
    let mut else_f = Vec::new();
    if lt {
        // then: x < y; else: y <= x.
        strictly_below(func, defs, x, y, at, &mut then_f);
        special::square_guard(func, defs, gi, k, x, y, &mut then_f);
        at_least(defs, func, y, x, 0, &mut then_f);
        at_most(func, defs, y, x, &mut else_f);
        at_least(defs, func, x, y, 1, &mut else_f);
    } else {
        // then: x <= y; else: y < x.
        at_most(func, defs, x, y, &mut then_f);
        at_least(defs, func, y, x, 1, &mut then_f);
        strictly_below(func, defs, y, x, at, &mut else_f);
        at_least(defs, func, x, y, 0, &mut else_f);
    }
    let later = &stmts[k + 1..];
    let redefined = |f: &Fact| {
        later
            .iter()
            .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if f.mentions(d.0)))
    };
    then_f.retain(|f| !redefined(f));
    else_f.retain(|f| !redefined(f));
    (then_f, else_f)
}

/// `lhs < rhs` holds.
fn strictly_below(
    func: &MirFunction,
    defs: &Defs,
    lhs: &Operand,
    rhs: &Operand,
    at: Option<(usize, usize)>,
    out: &mut Vec<Fact>,
) {
    let Some(i) = as_local(lhs) else {
        return;
    };
    out.push(Fact::Bounded(i.0));
    if let Some(k) = defs.const_value(func, rhs) {
        out.push(Fact::BelowConst(i.0, k));
    }
    for b in len_bounds(func, defs, rhs, at) {
        out.push(Fact::Below(i.0, b));
    }
}

/// `lhs <= rhs` holds.
fn at_most(func: &MirFunction, defs: &Defs, lhs: &Operand, rhs: &Operand, out: &mut Vec<Fact>) {
    let Some(i) = as_local(lhs) else {
        return;
    };
    let mut bounded = false;
    if let Some(k) = defs.const_value(func, rhs) {
        if k < i32::MAX as i64 {
            out.push(Fact::BelowConst(i.0, k + 1));
            bounded = true;
        }
    }
    for b in le_bounds(func, defs, rhs) {
        out.push(Fact::Below(i.0, b));
        bounded = true;
    }
    if bounded {
        out.push(Fact::Bounded(i.0));
    }
}

/// `lhs + slack > rhs` holds with `slack` 0 (`lhs > rhs`) or 1 (`lhs >= rhs`); records `lhs >= 0`
/// when `rhs` is a constant that implies it.
fn at_least(
    defs: &Defs,
    func: &MirFunction,
    lhs: &Operand,
    rhs: &Operand,
    slack: i64,
    out: &mut Vec<Fact>,
) {
    let Some(i) = as_local(lhs) else {
        return;
    };
    if defs
        .const_value(func, rhs)
        .is_some_and(|k| k + (1 - slack) >= 0)
    {
        out.push(Fact::NonNeg(i.0));
    }
}

/// Bounds `b` with `value(op) <= len(b)`. `at` is the guard position, which lets a length read in
/// the guard's own block tie to a local that is not otherwise stable.
pub(super) fn len_bounds(
    func: &MirFunction,
    defs: &Defs,
    op: &Operand,
    at: Option<(usize, usize)>,
) -> Vec<Bound> {
    match op {
        Operand::Copy(Place::Local(n)) => link_local(func, defs, n.0, at, 0),
        _ => match const_int(op) {
            Some(k) if k >= 0 => arrays_len_at_least(func, defs, k),
            _ => Vec::new(),
        },
    }
}

/// Bounds `b` with `value(op) < len(b)`.
pub(super) fn le_bounds(func: &MirFunction, defs: &Defs, op: &Operand) -> Vec<Bound> {
    if let Some(k) = const_int(op) {
        return if (0..i64::MAX).contains(&k) {
            arrays_len_at_least(func, defs, k + 1)
        } else {
            Vec::new()
        };
    }
    let Some(m) = as_local(op) else {
        return Vec::new();
    };
    match defs.sole_def(func, m.0) {
        Some((rv, _)) => match minus_const(rv) {
            Some((x, c)) if c >= 1 => len_bounds(func, defs, x, None),
            _ => match rv {
                Rvalue::Use(src @ Operand::Copy(Place::Local(_))) => le_bounds(func, defs, src),
                _ => Vec::new(),
            },
        },
        None => Vec::new(),
    }
}

/// `x - c` (also spelled `x + -c`) as `(x, c)`.
pub(super) fn minus_const(rv: &Rvalue) -> Option<(&Operand, i64)> {
    match rv {
        Rvalue::Binary(BinOp::Sub, x, k) | Rvalue::CheckedBinary(BinOp::Sub, x, k) => {
            Some((x, const_int(k)?))
        }
        Rvalue::Binary(BinOp::Add, x, k) | Rvalue::CheckedBinary(BinOp::Add, x, k) => {
            if let Some(c) = const_int(k) {
                Some((x, c.checked_neg()?))
            } else {
                Some((k, const_int(x)?.checked_neg()?))
            }
        }
        _ => None,
    }
}

fn link_local(
    func: &MirFunction,
    defs: &Defs,
    n: u32,
    at: Option<(usize, usize)>,
    depth: u32,
) -> Vec<Bound> {
    let mut out = Vec::new();
    if let Some((rv, pos)) = defs.sole_def(func, n) {
        let tied = |l: u32| defs.stable(l) || same_block_since(func, at, pos, l);
        match rv {
            Rvalue::ArrayLen(Operand::Copy(Place::Local(a))) if tied(a.0) => {
                out.push(Bound::Arr(a.0));
            }
            Rvalue::StrLen(s) | Rvalue::StrByteSize(s) => {
                if let Some(b) = str_base(s) {
                    let ok = match &b {
                        StrBase::Local(l) => tied(*l),
                        StrBase::Lit(_) => true,
                    };
                    if ok {
                        out.push(if matches!(rv, Rvalue::StrLen(_)) {
                            Bound::Unit(b)
                        } else {
                            Bound::Byte(b)
                        });
                    }
                }
            }
            Rvalue::Use(Operand::Copy(Place::Local(m))) if depth < 4 => {
                out.extend(link_local(func, defs, m.0, at, depth + 1));
            }
            Rvalue::Use(Operand::Const(crate::Const::Int(k))) if *k >= 0 => {
                out.extend(arrays_len_at_least(func, defs, *k));
            }
            _ => {}
        }
    }
    if defs.stable(n) {
        for (a, rv) in sole_array_news(func, defs) {
            if let Rvalue::ArrayNew {
                len: Operand::Copy(Place::Local(l)),
                ..
            } = rv
            {
                if l.0 == n {
                    out.push(Bound::Arr(a));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The length read at `def` and the guard at `at` share a block, and `l` is not reassigned between.
fn same_block_since(
    func: &MirFunction,
    at: Option<(usize, usize)>,
    def: (usize, usize),
    l: u32,
) -> bool {
    let Some((gb, gs)) = at else {
        return false;
    };
    gb == def.0
        && def.1 < gs
        && !func.blocks[gb].stmts[def.1 + 1..gs]
            .iter()
            .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if d.0 == l))
}

/// Arrays with exactly one `ArrayNew` definition. The RC inserter's `a = null` after the final
/// release does not count: no access can follow it without passing a new definition.
pub(super) fn sole_array_news<'f>(func: &'f MirFunction, defs: &Defs) -> Vec<(u32, &'f Rvalue)> {
    let mut nulls: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(Place::Local(a), rv @ Rvalue::ArrayNew { .. }) => {
                    out.push((a.0, rv));
                }
                Statement::Assign(Place::Local(a), Rvalue::Use(Operand::Const(Const::Null))) => {
                    *nulls.entry(a.0).or_default() += 1;
                }
                _ => {}
            }
        }
    }
    out.retain(|(a, _)| defs.count(*a) == 1 + nulls.get(a).copied().unwrap_or(0));
    out
}

/// Single-definition arrays allocated with a constant length of at least `k`.
pub(super) fn arrays_len_at_least(func: &MirFunction, defs: &Defs, k: i64) -> Vec<Bound> {
    sole_array_news(func, defs)
        .into_iter()
        .filter_map(|(a, rv)| match rv {
            Rvalue::ArrayNew { len, .. } => defs
                .const_value(func, len)
                .filter(|&v| v >= k)
                .map(|_| Bound::Arr(a)),
            _ => None,
        })
        .collect()
}

/// `x + 1` (either operand order) as `x`.
fn unit_increment_of(rv: &Rvalue) -> Option<Local> {
    let (Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b)) = rv else {
        return None;
    };
    match (as_local(a), as_local(b)) {
        (Some(x), None) if const_int(b) == Some(1) => Some(x),
        (None, Some(x)) if const_int(a) == Some(1) => Some(x),
        _ => None,
    }
}

/// `x - c` with `c >= 1` as `x`.
fn decrement_of(rv: &Rvalue) -> Option<Local> {
    let (x, c) = minus_const(rv)?;
    (c >= 1).then_some(())?;
    as_local(x)
}

fn nonneg_operand(op: &Operand, cand: &BTreeSet<u32>) -> bool {
    match op {
        Operand::Copy(Place::Local(l)) => cand.contains(&l.0),
        _ => const_int(op).is_some_and(|v| v >= 0),
    }
}

/// Locals every definition of which yields a value `>= 0`, as a greatest fixpoint so a counter
/// `i = 0; … i = i + 1` qualifies. A wrapping `x + 1` only preserves the sign when `x < v` was
/// established first (`bounded_incr`).
fn global_nonneg(
    func: &MirFunction,
    defs: &Defs,
    bounded_incr: &BTreeSet<(usize, usize)>,
) -> BTreeSet<u32> {
    let mut cand: BTreeSet<u32> = (0..func.locals.len() as u32)
        .filter(|&l| defs.count(l) > 0)
        .collect();
    for block in &func.blocks {
        if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
            cand.remove(&d.0);
        }
    }
    loop {
        let mut changed = false;
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                let Statement::Assign(Place::Local(d), rv) = stmt else {
                    continue;
                };
                if !cand.contains(&d.0) {
                    continue;
                }
                let bounded = bounded_incr.contains(&(bi, si));
                if !nonneg_rvalue(rv, &cand, bounded) {
                    cand.remove(&d.0);
                    changed = true;
                }
            }
        }
        if !changed {
            return cand;
        }
    }
}

fn nonneg_rvalue(rv: &Rvalue, cand: &BTreeSet<u32>, bounded_incr: bool) -> bool {
    match rv {
        Rvalue::Use(op) => nonneg_operand(op, cand),
        Rvalue::ArrayLen(_) | Rvalue::StrLen(_) | Rvalue::StrByteSize(_) => true,
        Rvalue::CheckedBinary(BinOp::Add | BinOp::Mul, a, b) => {
            nonneg_operand(a, cand) && nonneg_operand(b, cand)
        }
        Rvalue::Binary(BinOp::Add, a, b) => {
            if let (Some(x), Some(y)) = (const_int(a), const_int(b)) {
                return x >= 0 && y >= 0 && x + y <= i32::MAX as i64;
            }
            bounded_incr && unit_increment_of(rv).is_some_and(|x| cand.contains(&x.0))
        }
        Rvalue::Binary(BinOp::BitAnd, a, b) => {
            const_int(a).is_some_and(|v| v >= 0) || const_int(b).is_some_and(|v| v >= 0)
        }
        Rvalue::Binary(BinOp::Shr, a, _) => nonneg_operand(a, cand),
        Rvalue::Binary(BinOp::Div | BinOp::Rem, a, b) => {
            nonneg_operand(a, cand) && const_int(b).is_some_and(|v| v > 0)
        }
        _ => false,
    }
}

/// Decreasing induction variables: every definition is `len(b) - c` (`c >= 1`) or a copy of such a
/// value, or `i - c` taken while `i >= 0` (so it cannot wrap). `i < len(b)` then holds everywhere.
fn decreasing_below(
    func: &MirFunction,
    defs: &Defs,
    nonneg_decr: &BTreeSet<(usize, usize)>,
) -> BTreeSet<(u32, Bound)> {
    let mut per_local: BTreeMap<u32, Option<BTreeSet<Bound>>> = BTreeMap::new();
    let mut bad: BTreeSet<u32> = BTreeSet::new();
    for block in &func.blocks {
        if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
            bad.insert(d.0);
        }
    }
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if bad.contains(&d.0) {
                continue;
            }
            if decrement_of(rv) == Some(*d) {
                let checked = matches!(rv, Rvalue::CheckedBinary(..));
                if !checked && !nonneg_decr.contains(&(bi, si)) {
                    bad.insert(d.0);
                }
                continue;
            }
            let bounds: BTreeSet<Bound> = match minus_const(rv) {
                Some((x, c)) if c >= 1 && as_local(x) != Some(*d) => {
                    len_bounds(func, defs, x, None).into_iter().collect()
                }
                _ => match rv {
                    Rvalue::Use(src @ Operand::Copy(Place::Local(m))) if *m != *d => {
                        le_bounds(func, defs, src).into_iter().collect()
                    }
                    _ => BTreeSet::new(),
                },
            };
            if bounds.is_empty() {
                bad.insert(d.0);
                continue;
            }
            let slot = per_local.entry(d.0).or_insert(None);
            *slot = Some(match slot.take() {
                None => bounds,
                Some(prev) => prev.intersection(&bounds).cloned().collect(),
            });
        }
    }
    let mut out = BTreeSet::new();
    for (l, bounds) in per_local {
        if bad.contains(&l) {
            continue;
        }
        for b in bounds.into_iter().flatten() {
            out.insert((l, b));
        }
    }
    out
}
