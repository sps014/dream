//! Escape analysis over monomorphized MIR.
//!
//! Reference locals are grouped into alias classes: a copy, cast, `Move` or `Select` into a local
//! joins its sources' classes. Each class gets an [`Escape`] level:
//!
//! - [`Escape::No`]: every use reads through the reference (field, element, payload, tag,
//!   pointer compare) or adjusts its count;
//! - [`Escape::Arg`]: it is also passed to direct callees whose parameter summary keeps nothing;
//! - [`Escape::Global`]: something may hold it past the frame: a return, a store into anything but
//!   a local, an aggregate payload, an unknown, indirect, interface or `js` call, protocol dispatch
//!   (`to_string`, `hash_code`, `print`), `await`, a `ref` binding, or an escaping callee parameter.
//!
//! Parameter summaries are computed callees-first over the call graph's strongly connected
//! components ([`tarjan_scc`]): optimistic inside a component, iterated to a fixpoint. A callee
//! with no MIR body (intrinsic, import) keeps every argument.

use crate::passes::inline::graph::tarjan_scc;
use crate::{Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

pub(crate) type FnKey = (DefId, Vec<TypeId>);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Escape {
    No,
    Arg,
    Global,
}

/// Whether each parameter of each function instance may outlive a call.
#[derive(Default)]
pub(crate) struct ParamSummaries {
    params: IndexMap<FnKey, Vec<bool>>,
    /// Instances on a call cycle.
    recursive: HashSet<FnKey>,
}

impl ParamSummaries {
    pub(crate) fn compute(mir: &Mir, interner: &TypeInterner) -> Self {
        let index: HashMap<FnKey, usize> = mir
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| ((f.def, f.instance.clone()), i))
            .collect();
        let adj: Vec<Vec<usize>> = mir
            .functions
            .iter()
            .map(|f| {
                let mut out = Vec::new();
                for_each_callee(f, |k| {
                    if let Some(&j) = index.get(&k) {
                        out.push(j);
                    }
                });
                out
            })
            .collect();
        let mut sums = ParamSummaries::default();
        for scc in tarjan_scc(&adj) {
            if scc.len() > 1 || adj[scc[0]].contains(&scc[0]) {
                for &i in &scc {
                    let f = &mir.functions[i];
                    sums.recursive.insert((f.def, f.instance.clone()));
                }
            }
            for &i in &scc {
                let f = &mir.functions[i];
                sums.params
                    .insert((f.def, f.instance.clone()), vec![f.is_async; f.params.len()]);
            }
            loop {
                let mut changed = false;
                for &i in &scc {
                    let f = &mir.functions[i];
                    if f.is_async {
                        continue;
                    }
                    let esc = LocalEscape::analyze(f, interner, &sums);
                    let now: Vec<bool> = f
                        .params
                        .iter()
                        .map(|&p| esc.of(p) == Escape::Global)
                        .collect();
                    let cur = sums
                        .params
                        .get_mut(&(f.def, f.instance.clone()))
                        .expect("summary seeded for every SCC member");
                    if *cur != now {
                        *cur = now;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        sums
    }

    pub(crate) fn is_recursive(&self, def: DefId, args: &[TypeId]) -> bool {
        self.recursive.contains(&(def, args.to_vec()))
    }

    pub(crate) fn param_escapes(&self, def: DefId, args: &[TypeId], i: usize) -> bool {
        self.params
            .get(&(def, args.to_vec()))
            .and_then(|p| p.get(i).copied())
            .unwrap_or(true)
    }
}

fn for_each_callee(f: &MirFunction, mut out: impl FnMut(FnKey)) {
    for b in &f.blocks {
        for s in &b.stmts {
            match s {
                Statement::Call { callee, .. }
                | Statement::Assign(_, Rvalue::Call { callee, .. }) => {
                    out((callee.def, callee.args.clone()))
                }
                Statement::Assign(
                    _,
                    Rvalue::New {
                        ctor: Some(c), ..
                    },
                ) => out((c.def, vec![])),
                _ => {}
            }
        }
    }
}

/// Per-function alias classes and their escape levels.
pub(crate) struct LocalEscape {
    root: Vec<u32>,
    level: Vec<Escape>,
}

impl LocalEscape {
    pub(crate) fn analyze(f: &MirFunction, interner: &TypeInterner, sums: &ParamSummaries) -> Self {
        let n = f.locals.len();
        let tracked = |l: Local| interner.is_reference(f.local_ty(l));
        let mut uf = UnionFind::new(n);
        for b in &f.blocks {
            for s in &b.stmts {
                if let Statement::Assign(Place::Local(d), rv) = s {
                    if tracked(*d) {
                        alias_sources(rv, |src| {
                            if tracked(src) {
                                uf.union(d.0, src.0);
                            }
                        });
                    }
                }
            }
        }
        let root: Vec<u32> = (0..n as u32).map(|l| uf.find(l)).collect();
        let mut level = vec![Escape::No; n];
        let mut mark = |l: Local, e: Escape| {
            if (l.0 as usize) < n && tracked(l) {
                let r = root[l.0 as usize] as usize;
                level[r] = level[r].max(e);
            }
        };
        for (i, d) in f.locals.iter().enumerate() {
            if d.is_ref {
                mark(Local(i as u32), Escape::Global);
            }
        }
        for b in &f.blocks {
            for s in &b.stmts {
                stmt_uses(f, interner, s, sums, &mut mark);
            }
            term_uses(&b.terminator, &mut mark);
        }
        LocalEscape { root, level }
    }

    pub(crate) fn of(&self, l: Local) -> Escape {
        self.level[self.root[l.0 as usize] as usize]
    }

    #[cfg(test)]
    pub(crate) fn same_class(&self, a: Local, b: Local) -> bool {
        self.root[a.0 as usize] == self.root[b.0 as usize]
    }

    /// Every local in `l`'s class, ascending.
    pub(crate) fn class(&self, l: Local) -> Vec<Local> {
        let r = self.root[l.0 as usize];
        (0..self.root.len() as u32)
            .filter(|&m| self.root[m as usize] == r)
            .map(Local)
            .collect()
    }
}

/// Locals whose reference `rv` copies into its destination.
fn alias_sources(rv: &Rvalue, mut out: impl FnMut(Local)) {
    match rv {
        Rvalue::Use(op) | Rvalue::Cast(op, _, _) => local_of(op).into_iter().for_each(out),
        Rvalue::Move { src, .. } => out(*src),
        Rvalue::Select {
            then_val, else_val, ..
        } => {
            local_of(then_val).into_iter().for_each(&mut out);
            local_of(else_val).into_iter().for_each(out);
        }
        _ => {}
    }
}

fn local_of(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn call_args(
    def: DefId,
    targs: &[TypeId],
    args: &[Operand],
    offset: usize,
    sums: &ParamSummaries,
    mark: &mut impl FnMut(Local, Escape),
) {
    for (i, a) in args.iter().enumerate() {
        if let Some(l) = local_of(a) {
            let e = if sums.param_escapes(def, targs, i + offset) {
                Escape::Global
            } else {
                Escape::Arg
            };
            mark(l, e);
        }
    }
}

fn all_global<'a>(ops: impl IntoIterator<Item = &'a Operand>, mark: &mut impl FnMut(Local, Escape)) {
    for op in ops {
        if let Some(l) = local_of(op) {
            mark(l, Escape::Global);
        }
    }
}

fn js_operands<'a>(
    target: &'a Operand,
    via: &'a Option<Operand>,
    method: &'a Option<Operand>,
    args: &'a [(Operand, TypeId)],
) -> impl Iterator<Item = &'a Operand> {
    std::iter::once(target)
        .chain(via)
        .chain(method)
        .chain(args.iter().map(|(a, _)| a))
}

fn stmt_uses(
    f: &MirFunction,
    interner: &TypeInterner,
    s: &Statement,
    sums: &ParamSummaries,
    mark: &mut impl FnMut(Local, Escape),
) {
    match s {
        Statement::Assign(place, rv) => {
            // A reference cast to an integer (funcbox environments) is a pointer the class
            // can no longer see.
            let into_local = match place {
                Place::Local(d) => {
                    !f.locals[d.0 as usize].is_ref && interner.is_reference(f.local_ty(*d))
                }
                _ => false,
            };
            if !into_local {
                alias_sources(rv, |l| mark(l, Escape::Global));
            }
            let dest = match place {
                Place::Local(d) => Some(*d),
                _ => None,
            };
            rvalue_uses(rv, dest, sums, mark);
        }
        Statement::Call { callee, args } => {
            call_args(callee.def, &callee.args, args, 0, sums, mark)
        }
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => all_global(js_operands(target, via, method, args), mark),
        Statement::InterfaceCall { receiver, args, .. } => {
            all_global(std::iter::once(receiver).chain(args), mark)
        }
        Statement::IndirectCall { target, args, .. } => {
            all_global(std::iter::once(target).chain(args), mark)
        }
        Statement::Print { arg, .. } => all_global([arg], mark),
        Statement::ArrayElemsCopy { dst, src, .. } => all_global([dst, src], mark),
        Statement::ArrayElemsFill { dst, .. } => all_global([dst], mark),
        Statement::ForceFree(op) => all_global([op], mark),
        Statement::SimdV128 {
            dest, lhs, rhs, splat_rhs, ..
        } => all_global([dest, lhs, rhs].iter().copied().chain(splat_rhs), mark),
        Statement::Retain(_)
        | Statement::Release(_)
        | Statement::ReleaseUnique(_)
        | Statement::Panic(_)
        | Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::LockAcquire(_)
        | Statement::LockRelease(_)
        | Statement::DeferEnter
        | Statement::DeferLeave(_)
        | Statement::RegionEnter
        | Statement::RegionLeave
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => {}
    }
}

fn rvalue_uses(
    rv: &Rvalue,
    dest: Option<Local>,
    sums: &ParamSummaries,
    mark: &mut impl FnMut(Local, Escape),
) {
    match rv {
        Rvalue::Call { callee, args } => call_args(callee.def, &callee.args, args, 0, sums, mark),
        Rvalue::New {
            ctor: Some(c),
            args,
            ..
        } => {
            call_args(c.def, &[], args, 1, sums, mark);
            if let Some(d) = dest {
                if sums.param_escapes(c.def, &[], 0) {
                    mark(d, Escape::Global);
                }
            }
        }
        Rvalue::Tuple { elems: ops, .. }
        | Rvalue::ArrayLit { elems: ops, .. }
        | Rvalue::UnionNew { args: ops, .. } => all_global(ops, mark),
        Rvalue::IndirectCall { target, args, .. } => {
            all_global(std::iter::once(target).chain(args), mark)
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            all_global(std::iter::once(receiver).chain(args), mark)
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => all_global(js_operands(target, via, method, args), mark),
        Rvalue::HashCode(op)
        | Rvalue::ToString(op)
        | Rvalue::ToBytes { value: op, .. }
        | Rvalue::FromBytes { bytes: op, .. }
        | Rvalue::ArrayRealloc { array: op, .. } => all_global([op], mark),
        Rvalue::Binary(op, a, b) | Rvalue::CheckedBinary(op, a, b) if !op.is_comparison() => {
            all_global([a, b], mark)
        }
        Rvalue::Unary(_, op) | Rvalue::CheckedNeg(op) => all_global([op], mark),
        Rvalue::Use(_)
        | Rvalue::Cast(..)
        | Rvalue::Move { .. }
        | Rvalue::Select { .. }
        | Rvalue::New { ctor: None, .. }
        | Rvalue::Binary(..)
        | Rvalue::CheckedBinary(..)
        | Rvalue::StrLen(_)
        | Rvalue::StrByteSize(_)
        | Rvalue::CharAt(..)
        | Rvalue::ByteAt(..)
        | Rvalue::ArrayNew { .. }
        | Rvalue::Concat(_)
        | Rvalue::ConcatInt { .. }
        | Rvalue::EnumName { .. }
        | Rvalue::FuncRef(_)
        | Rvalue::ArrayLen(_)
        | Rvalue::Discriminant { .. }
        | Rvalue::UnionField { .. }
        | Rvalue::IsType(..)
        | Rvalue::TypeName(_) => {}
    }
}

fn term_uses(t: &Terminator, mark: &mut impl FnMut(Local, Escape)) {
    match t {
        Terminator::Return(Some(op)) | Terminator::AsyncComplete(Some(op)) => {
            all_global([op], mark)
        }
        Terminator::Await { future, .. } => all_global([future], mark),
        Terminator::TailCall { args, .. } => all_global(args, mark),
        Terminator::Goto(_)
        | Terminator::If { .. }
        | Terminator::Switch { .. }
        | Terminator::Return(None)
        | Terminator::AsyncComplete(None)
        | Terminator::Unreachable => {}
    }
}

struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n as u32).collect(),
        }
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let p = self.parent[x as usize];
            self.parent[x as usize] = self.parent[p as usize];
            x = p;
        }
        x
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra.max(rb) as usize] = ra.min(rb);
        }
    }
}
