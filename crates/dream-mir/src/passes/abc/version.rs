//! Loop versioning for bounds checks the static facts cannot remove.
//!
//! An innermost loop `while i < n { … a[i] … i = i + 1 }` whose bound `n` is not tied to `a`'s
//! length gets a guarded clone: on entry, `n <= a.length && i >= 0` (for every such invariant array
//! `a`) selects the clone, where those accesses are unchecked; otherwise the original loop runs
//! with its checks. Inside the clone, `i < n <= a.length` holds at every access that no increment of
//! `i` can precede within the iteration, and `i` only grows from a non-negative start.
//!
//! An offset index `a[t]` with `t = base + i` formed earlier in the same block, `base` invariant,
//! is guarded by `base >= 0 && n <= a.length - base` instead.

use super::facts::{self, Defs};
use super::{
    Access, as_local, block_id, const_int, visit_stmt_accesses, visit_terminator_accesses,
};
use crate::passes::cfg::{DomTree, NaturalLoop};
use crate::{
    BasicBlock, BinOp, BlockId, Const, Local, LocalDecl, MirFunction, Operand, Place, Rvalue,
    Statement, Terminator,
};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet};

/// Largest loop (blocks / statements) worth duplicating.
const MAX_CLONE_BLOCKS: usize = 12;
const MAX_CLONE_STMTS: usize = 96;
/// Beyond this, the function is already large enough that code growth is not worth it.
const MAX_FUNC_BLOCKS: usize = 2048;
const MAX_ARRAYS: usize = 4;

/// One guarded index shape: `array[iv]` (`offset` is `None`) or `array[t]` with `t = base + iv`
/// (`offset` is `(t, base)`), `base` loop-invariant.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Target {
    array: Local,
    offset: Option<(Local, Local)>,
}

impl Target {
    fn index(&self, iv: Local) -> Local {
        self.offset.map_or(iv, |(t, _)| t)
    }
}

struct Plan {
    preheader: usize,
    iv: Local,
    bound: Operand,
    targets: Vec<Target>,
    /// Accesses proven at `(block, Some(stmt))`, or at `(block, None)` for the terminator.
    sites: BTreeMap<(usize, Option<usize>), Vec<Target>>,
}

/// Versions at most one loop; the pass manager's fixpoint picks up the rest.
pub(super) fn version_one_loop(
    func: &mut MirFunction,
    interner: &TypeInterner,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    if func.blocks.len() > MAX_FUNC_BLOCKS
        || func.blocks.iter().any(|b| {
            matches!(
                b.terminator,
                Terminator::Await { .. } | Terminator::AsyncComplete(_)
            )
        })
    {
        return false;
    }
    let loops = analyses.natural_loops(func);
    if loops.is_empty() {
        return false;
    }
    let headers: BTreeSet<BlockId> = loops.iter().map(|l| l.header).collect();
    let preds = analyses.predecessors(func);
    let dom = analyses.dominators(func);
    let defs = Defs::new(func);
    for l in loops.iter() {
        if l.body.iter().any(|b| *b != l.header && headers.contains(b)) {
            continue;
        }
        if let Some(plan) = plan_loop(func, interner, &defs, &preds, &dom, l, analyses) {
            apply(func, interner, l, plan);
            return true;
        }
    }
    false
}

fn plan_loop(
    func: &MirFunction,
    interner: &TypeInterner,
    defs: &Defs,
    preds: &[Vec<BlockId>],
    dom: &DomTree,
    l: &NaturalLoop,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> Option<Plan> {
    let h = l.header.0 as usize;
    let body: BTreeSet<usize> = l.body.iter().map(|b| b.0 as usize).collect();
    if body.len() > MAX_CLONE_BLOCKS
        || body
            .iter()
            .map(|&b| func.blocks[b].stmts.len())
            .sum::<usize>()
            > MAX_CLONE_STMTS
    {
        return None;
    }
    let Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk,
        else_blk,
    } = &func.blocks[h].terminator
    else {
        return None;
    };
    let s = then_blk.0 as usize;
    if !body.contains(&s) || body.contains(&(else_blk.0 as usize)) || preds[s] != [l.header] {
        return None;
    }
    let (iv, bound) = header_compare(func, h, *c)?;
    let int = interner.int();
    if func.local_ty(iv) != int {
        return None;
    }
    if let Some(n) = as_local(&bound) {
        if func.local_ty(n) != int || defs.blocks(n.0).iter().any(|b| body.contains(b)) {
            return None;
        }
    } else {
        const_int(&bound)?;
    }
    let outside: Vec<usize> = preds[h]
        .iter()
        .map(|p| p.0 as usize)
        .filter(|p| !body.contains(p))
        .collect();
    let [preheader] = outside.as_slice() else {
        return None;
    };
    let preheader = *preheader;
    if !matches!(func.blocks[preheader].terminator, Terminator::Goto(t) if t == l.header) {
        return None;
    }
    let reach = reaching(preds, preheader);
    let available = |x: Local| {
        let reaching_defs: Vec<usize> = defs
            .blocks(x.0)
            .iter()
            .copied()
            .filter(|&d| reach.contains(&d))
            .collect();
        (defs.is_param(x.0) || !reaching_defs.is_empty())
            && reaching_defs.iter().all(|&d| {
                dom.dominates(block_id(d), block_id(preheader)) && !assigns_null(&func.blocks[d], x)
            })
    };
    if !available(iv) || as_local(&bound).is_some_and(|n| !available(n)) {
        return None;
    }
    for &b in &body {
        if !loop_block_clonable(&func.blocks[b]) {
            return None;
        }
        for stmt in &func.blocks[b].stmts {
            if let Statement::Assign(Place::Local(d), rv) = stmt
                && *d == iv
                && !is_unit_increment(rv, iv)
            {
                return None;
            }
        }
    }
    let redef_in = redefined_since_header(func, &body, h, s, iv, analyses);
    let invariant = |a: Local| !defs.blocks(a.0).iter().any(|b| body.contains(b)) && available(a);
    let int_base = |x: Local| func.local_ty(x) == int && invariant(x);
    let mut sites: BTreeMap<(usize, Option<usize>), Vec<Target>> = BTreeMap::new();
    for &b in &body {
        if b == h {
            continue;
        }
        let mut redef = redef_in[&b];
        // `t = base + iv` computed earlier in this block from this iteration's `iv`.
        let mut offsets: BTreeMap<Local, Local> = BTreeMap::new();
        let block = &func.blocks[b];
        for (si, stmt) in block.stmts.iter().enumerate() {
            if !redef {
                let mut hits = Vec::new();
                visit_stmt_accesses(stmt, &mut |acc| {
                    if let Some(t) = versionable(&acc, iv, &offsets)
                        && invariant(t.array)
                    {
                        hits.push(t);
                    }
                });
                if !hits.is_empty() {
                    sites.insert((b, Some(si)), hits);
                }
                if let Some((t, base)) = offset_def(stmt, iv)
                    && defs.count(t.0) == 1
                    && func.local_ty(t) == int
                    && int_base(base)
                {
                    offsets.insert(t, base);
                }
            }
            if matches!(stmt, Statement::Assign(Place::Local(d), _) if *d == iv) {
                redef = true;
            }
        }
        if !redef {
            let mut hits = Vec::new();
            visit_terminator_accesses(&block.terminator, &mut |acc| {
                if let Some(t) = versionable(&acc, iv, &offsets)
                    && invariant(t.array)
                {
                    hits.push(t);
                }
            });
            if !hits.is_empty() {
                sites.insert((b, None), hits);
            }
        }
    }
    let targets: BTreeSet<Target> = sites.values().flatten().copied().collect();
    if targets.is_empty() || targets.len() > MAX_ARRAYS {
        return None;
    }
    if const_int(&bound).is_some()
        && facts::sole_array_news(func, defs).iter().any(|(a, rv)| {
            targets
                .iter()
                .any(|t| t.offset.is_none() && t.array == Local(*a))
                && match rv {
                    Rvalue::ArrayNew { len, .. } => defs.const_value(func, len).is_some(),
                    Rvalue::ArrayLit { .. } => true,
                    _ => false,
                }
        })
    {
        return None;
    }
    Some(Plan {
        preheader,
        iv,
        bound,
        targets: targets.into_iter().collect(),
        sites,
    })
}

/// `t = base + iv` (either operand order) as `(t, base)`.
fn offset_def(stmt: &Statement, iv: Local) -> Option<(Local, Local)> {
    let Statement::Assign(
        Place::Local(t),
        Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b),
    ) = stmt
    else {
        return None;
    };
    let (a, b) = (as_local(a)?, as_local(b)?);
    match (a == iv, b == iv) {
        (true, false) => Some((*t, b)),
        (false, true) => Some((*t, a)),
        _ => None,
    }
}

/// `c = i < n` (or `n > i`) as the header's last definition of `c`, with neither side redefined
/// before the branch.
fn header_compare(func: &MirFunction, h: usize, c: Local) -> Option<(Local, Operand)> {
    let stmts = &func.blocks[h].stmts;
    let k = stmts
        .iter()
        .rposition(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == c))?;
    let (iv, bound) = match &stmts[k] {
        Statement::Assign(_, Rvalue::Binary(BinOp::Lt, a, b)) => (as_local(a)?, b.clone()),
        Statement::Assign(_, Rvalue::Binary(BinOp::Gt, a, b)) => (as_local(b)?, a.clone()),
        _ => return None,
    };
    let bound_local = as_local(&bound);
    if stmts[k + 1..].iter().any(|s| {
        matches!(s, Statement::Assign(Place::Local(d), _) if *d == iv || Some(*d) == bound_local)
    }) {
        return None;
    }
    Some((iv, bound))
}

/// Blocks from which `to` is reachable (including `to`).
fn reaching(preds: &[Vec<BlockId>], to: usize) -> BTreeSet<usize> {
    let mut seen = BTreeSet::from([to]);
    let mut work = vec![to];
    while let Some(b) = work.pop() {
        for p in &preds[b] {
            if seen.insert(p.0 as usize) {
                work.push(p.0 as usize);
            }
        }
    }
    seen
}

/// A released/moved-from local is nulled; the entry guard must not read `a.length` through it.
fn assigns_null(block: &BasicBlock, x: Local) -> bool {
    block.stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Const(Const::Null))) if *d == x
        )
    })
}

fn is_unit_increment(rv: &Rvalue, iv: Local) -> bool {
    let (Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b)) = rv else {
        return false;
    };
    (as_local(a) == Some(iv) && const_int(b) == Some(1))
        || (as_local(b) == Some(iv) && const_int(a) == Some(1))
}

fn loop_block_clonable(block: &BasicBlock) -> bool {
    matches!(
        block.terminator,
        Terminator::Goto(_) | Terminator::If { .. } | Terminator::Switch { .. }
    ) && !block.stmts.iter().any(|s| {
        matches!(
            s,
            Statement::DeferEnter
                | Statement::DeferLeave(_)
                | Statement::RegionEnter
                | Statement::RegionLeave
                | Statement::LockAcquire(_)
                | Statement::LockRelease(_)
        )
    })
}

fn versionable(acc: &Access<'_>, iv: Local, offsets: &BTreeMap<Local, Local>) -> Option<Target> {
    let Access::Index {
        base,
        index,
        unchecked: false,
    } = acc
    else {
        return None;
    };
    let index = as_local(index)?;
    if index == iv {
        return Some(Target {
            array: *base,
            offset: None,
        });
    }
    offsets.get(&index).map(|&b| Target {
        array: *base,
        offset: Some((index, b)),
    })
}

/// Whether `iv` may already have been incremented this iteration on entry to each non-header loop
/// block. The body minus the header is acyclic (innermost loop), so one pass in reverse postorder
/// settles it.
fn redefined_since_header(
    func: &MirFunction,
    body: &BTreeSet<usize>,
    h: usize,
    s: usize,
    iv: Local,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> BTreeMap<usize, bool> {
    let defines = |b: usize| {
        func.blocks[b]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::Assign(Place::Local(d), _) if *d == iv))
    };
    let preds = analyses.predecessors(func);
    let mut out: BTreeMap<usize, bool> = BTreeMap::new();
    for b in analyses.reverse_postorder(func).iter() {
        let b = b.0 as usize;
        if !body.contains(&b) || b == h {
            continue;
        }
        let v = b != s
            && preds[b].iter().any(|p| {
                let p = p.0 as usize;
                body.contains(&p) && p != h && (out.get(&p).copied().unwrap_or(true) || defines(p))
            });
        out.insert(b, v);
    }
    for &b in body {
        out.entry(b).or_insert(true);
    }
    out
}

fn apply(func: &mut MirFunction, interner: &TypeInterner, l: &NaturalLoop, plan: Plan) {
    let base = func.blocks.len();
    let map: BTreeMap<usize, usize> = l
        .body
        .iter()
        .enumerate()
        .map(|(k, b)| (b.0 as usize, base + k))
        .collect();
    for &old in map.keys() {
        let mut block = func.blocks[old].clone();
        remap_successors(&mut block.terminator, &map);
        for (si, stmt) in block.stmts.iter_mut().enumerate() {
            if let Some(proven) = plan.sites.get(&(old, Some(si))) {
                mark_stmt(stmt, proven, plan.iv);
            }
        }
        if let Some(proven) = plan.sites.get(&(old, None)) {
            mark_terminator(&mut block.terminator, proven, plan.iv);
        }
        func.blocks.push(block);
    }
    let header = l.header;
    let clone_header = block_id(map[&(header.0 as usize)]);
    let mut new_local = |ty| {
        func.locals.push(LocalDecl {
            ty,
            name: None,
            is_ref: false,
            is_take: false,
            is_cursor: false,
            manual_drop: false,
            borrows_refs: false,
        });
        Local(func.locals.len() as u32 - 1)
    };
    let int = interner.int();
    let boolean = interner.bool();
    let guard = |stmts: Vec<Statement>, ok: Local| BasicBlock {
        stmts,
        terminator: Terminator::If {
            cond: Operand::Copy(Place::Local(ok)),
            then_blk: header,
            else_blk: header,
        },
    };
    let non_negative = |x: Local, ok: Local| {
        Statement::Assign(
            Place::Local(ok),
            Rvalue::Binary(
                BinOp::Ge,
                Operand::Copy(Place::Local(x)),
                Operand::Const(Const::Int(0)),
            ),
        )
    };
    let mut guards: Vec<BasicBlock> = Vec::new();
    let mut bases: BTreeSet<Local> = BTreeSet::new();
    for t in &plan.targets {
        let len = new_local(int);
        let ok = new_local(boolean);
        let mut stmts = vec![Statement::Assign(
            Place::Local(len),
            Rvalue::ArrayLen(Operand::Copy(Place::Local(t.array))),
        )];
        // With `base >= 0` checked separately and `len >= 0`, `len - base` cannot wrap, and
        // `0 <= iv < bound <= len - base` keeps `base + iv` in `[0, len)` without wrapping.
        let room = match t.offset {
            None => len,
            Some((_, base)) => {
                bases.insert(base);
                let room = new_local(int);
                stmts.push(Statement::Assign(
                    Place::Local(room),
                    Rvalue::Binary(
                        BinOp::Sub,
                        Operand::Copy(Place::Local(len)),
                        Operand::Copy(Place::Local(base)),
                    ),
                ));
                room
            }
        };
        stmts.push(Statement::Assign(
            Place::Local(ok),
            Rvalue::Binary(
                BinOp::Le,
                plan.bound.clone(),
                Operand::Copy(Place::Local(room)),
            ),
        ));
        guards.push(guard(stmts, ok));
    }
    for base in bases {
        let ok = new_local(boolean);
        guards.push(guard(vec![non_negative(base, ok)], ok));
    }
    let start_ok = new_local(boolean);
    guards.push(guard(vec![non_negative(plan.iv, start_ok)], start_ok));
    let first_guard = func.blocks.len();
    let count = guards.len();
    for (k, mut g) in guards.into_iter().enumerate() {
        let next = if k + 1 == count {
            clone_header
        } else {
            block_id(first_guard + k + 1)
        };
        if let Terminator::If { then_blk, .. } = &mut g.terminator {
            *then_blk = next;
        }
        func.blocks.push(g);
    }
    func.blocks[plan.preheader].terminator = Terminator::Goto(block_id(first_guard));
}

fn remap_successors(t: &mut Terminator, map: &BTreeMap<usize, usize>) {
    let fix = |b: &mut BlockId| {
        if let Some(&n) = map.get(&(b.0 as usize)) {
            *b = block_id(n);
        }
    };
    match t {
        Terminator::Goto(b) => fix(b),
        Terminator::If {
            then_blk, else_blk, ..
        } => {
            fix(then_blk);
            fix(else_blk);
        }
        Terminator::Switch {
            targets, default, ..
        } => {
            for (_, b) in targets.iter_mut() {
                fix(b);
            }
            fix(default);
        }
        _ => {}
    }
}

fn mark_place(place: &mut Place, targets: &[Target], iv: Local) {
    if let Place::Index {
        base,
        index,
        unchecked,
    } = place
    {
        if let Some(i) = as_local(index)
            && targets.iter().any(|t| t.array == *base && t.index(iv) == i)
        {
            *unchecked = true;
        }
        mark_operand(index, targets, iv);
    }
}

fn mark_operand(op: &mut Operand, targets: &[Target], iv: Local) {
    if let Operand::Copy(p) = op {
        mark_place(p, targets, iv);
    }
}

fn mark_stmt(stmt: &mut Statement, targets: &[Target], iv: Local) {
    match stmt {
        Statement::Assign(place, rv) => {
            mark_place(place, targets, iv);
            for op in rvalue_operands_mut(rv) {
                mark_operand(op, targets, iv);
            }
        }
        Statement::Call { args, .. }
        | Statement::IndirectCall { args, .. }
        | Statement::InterfaceCall { args, .. } => {
            for a in args {
                mark_operand(a, targets, iv);
            }
        }
        _ => {}
    }
}

fn mark_terminator(t: &mut Terminator, targets: &[Target], iv: Local) {
    match t {
        Terminator::If { cond: o, .. } | Terminator::Switch { value: o, .. } => {
            mark_operand(o, targets, iv)
        }
        _ => {}
    }
}

fn rvalue_operands_mut(rv: &mut Rvalue) -> Vec<&mut Operand> {
    match rv {
        Rvalue::Use(o) | Rvalue::Unary(_, o) | Rvalue::CheckedNeg(o) | Rvalue::ArrayLen(o) => {
            vec![o]
        }
        Rvalue::Binary(_, a, b) | Rvalue::CheckedBinary(_, a, b) => vec![a, b],
        Rvalue::CharAt(a, b, _) | Rvalue::ByteAt(a, b, _) => vec![a, b],
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => vec![cond, then_val, else_val],
        Rvalue::Call { args, .. } | Rvalue::New { args, .. } => args.iter_mut().collect(),
        Rvalue::InterfaceCall { receiver, args, .. } => {
            let mut v = vec![receiver];
            v.extend(args.iter_mut());
            v
        }
        _ => Vec::new(),
    }
}
