//! Borrow inference for sink parameters.
//!
//! Every unmarked RC parameter is a sink (`take`): a caller that still needs its argument
//! retains it for the callee, and the callee releases it at scope exit. When the callee only
//! reads the parameter, that pair buys nothing. This pass flips such parameters to borrowed
//! (`is_take = false` in the callee, `take_params[i] = false` at every direct call site) before
//! [`super::RcInsertion`], which then emits neither half.
//!
//! A parameter is flipped only when all of these hold:
//!
//! - **Read-only**: it (or a local copy of it) is never rebound, stored into a slot, returned,
//!   captured, moved, awaited, passed to an indirect / interface / `js` call, or passed as a sink
//!   to a parameter that is not itself flipped (a fixpoint over the call graph).
//! - **Every call is known**: not async, not a constructor, not address-taken or an interface
//!   slot (those already use the +0 ABI, see `funcbox_abi`), and called at least once directly.
//! - **Destruction timing is unchanged**: an owned local at its last use is released right
//!   after a borrowing call, where the callee's frame used to release the sink. A caller's own
//!   sink parameter at its last use has no such release point, so it qualifies only when that
//!   parameter is flipped as well.
//! - **The callee cannot free a +0 argument**: when an argument may not be owned by the caller
//!   (a slot snapshot, a borrowed parameter, a heap operand), the callee's [`ModRefTable`]
//!   summary (and every `del`) must not overwrite any slot whose type can reach the parameter's.
//! - **Nothing observes the difference**: the callee does not read refcounts or live counts,
//!   and no class reachable from the parameter type has a `del`.

use super::rc::modref::{strong_children, Known, ModRef, ModRefTable};
use super::ModulePass;
use crate::{Callee, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::{IndexMap, IndexSet};
use std::collections::{BTreeSet, HashSet};

pub struct ParamModes;

type FnKey = (DefId, Vec<TypeId>);
type Slot = (FnKey, usize);

impl ModulePass for ParamModes {
    fn name(&self) -> &'static str {
        "param-modes"
    }

    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool {
        let modref = ModRefTable::compute(mir, interner);
        let flips = infer(mir, interner, &modref);
        if flips.is_empty() {
            return false;
        }
        for f in &mut mir.functions {
            let key = (f.def, f.instance.clone());
            for (k, pos) in &flips {
                if *k == key {
                    let p = f.params[*pos];
                    f.locals[p.0 as usize].is_take = false;
                }
            }
        }
        for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
            for callee in direct_callees_mut(f) {
                let key = (callee.def, callee.args.clone());
                for (pos, take) in callee.take_params.iter_mut().enumerate() {
                    if flips.contains(&(key.clone(), pos)) {
                        *take = false;
                    }
                }
            }
        }
        true
    }
}

fn infer(mir: &Mir, interner: &TypeInterner, modref: &ModRefTable) -> IndexSet<Slot> {
    let by_key: IndexMap<FnKey, usize> = mir
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.def, f.instance.clone()), i))
        .collect();
    let (opaque_defs, opaque_fns) = opaque_targets(mir);
    let reach = Reach {
        interner,
        layouts: &mir.layouts,
    };

    // Each candidate maps to the slots that must be flipped with it.
    let mut candidates: IndexMap<Slot, Vec<Slot>> = IndexMap::new();
    for f in &mir.functions {
        let key = (f.def, f.instance.clone());
        if f.is_async
            || opaque_defs.contains(&f.def)
            || opaque_fns.contains(&key)
            || modref.call_def(f.def, &f.instance).observes_rc()
        {
            continue;
        }
        for (pos, p) in f.params.iter().enumerate() {
            let d = &f.locals[p.0 as usize];
            if !d.is_take
                || !interner.is_reference(d.ty)
                || interner.is_shared_type(d.ty)
                || modref.may_run_del(d.ty, interner, &mir.layouts)
            {
                continue;
            }
            if let Some(deps) = read_only_deps(f, p.0, &by_key) {
                candidates.insert((key.clone(), pos), deps);
            }
        }
    }
    if candidates.is_empty() {
        return IndexSet::new();
    }

    let mut rejected: HashSet<Slot> = HashSet::new();
    let mut called: HashSet<Slot> = HashSet::new();
    let mut hazard_checked: IndexSet<Slot> = IndexSet::new();
    let mut extra_deps: Vec<(Slot, Slot)> = Vec::new();
    for f in mir.functions.iter().chain(mir.polls.iter()) {
        let caller = (f.def, f.instance.clone());
        let caller_slot = |l: u32| -> Option<Slot> {
            let pos = f.params.iter().position(|p| p.0 == l)?;
            let slot = (caller.clone(), pos);
            candidates.contains_key(&slot).then_some(slot)
        };
        CallerArgs::new(f).visit(f, |callee, at, pos, arg| {
            let slot = ((callee.def, callee.args.clone()), pos);
            if !candidates.contains_key(&slot) {
                return;
            }
            called.insert(slot.clone());
            match at.classify(f, arg, &caller_slot) {
                ArgKind::Owned => {}
                ArgKind::MaybeUnowned => {
                    hazard_checked.insert(slot);
                }
                ArgKind::WithCaller(dep) => {
                    hazard_checked.insert(slot.clone());
                    extra_deps.push((slot, dep));
                }
                ArgKind::Reject => {
                    rejected.insert(slot);
                }
            }
        });
    }
    for (slot, dep) in extra_deps {
        if let Some(deps) = candidates.get_mut(&slot) {
            deps.push(dep);
        }
    }
    for slot in &hazard_checked {
        let (key, pos) = slot;
        let callee = &mir.functions[by_key[key]];
        let ty = callee.locals[callee.params[*pos].0 as usize].ty;
        let drops = |m: &ModRef| match m {
            ModRef::Top => true,
            ModRef::Known(k) => reach.may_drop(k, ty),
        };
        if drops(&modref.call_def(key.0, &key.1)) || drops(modref.del()) {
            rejected.insert(slot.clone());
        }
    }

    let mut flips: IndexSet<Slot> = candidates
        .keys()
        .filter(|s| called.contains(*s) && !rejected.contains(*s))
        .cloned()
        .collect();
    loop {
        let before = flips.len();
        let kept: IndexSet<Slot> = flips
            .iter()
            .filter(|s| candidates[*s].iter().all(|d| flips.contains(d)))
            .cloned()
            .collect();
        flips = kept;
        if flips.len() == before {
            break;
        }
    }
    flips
}

/// Defs called as constructors or through `js`, and functions reached through a `fun` value or a
/// tail call (whose frame teardown `tco` reasons about with the original modes).
fn opaque_targets(mir: &Mir) -> (BTreeSet<DefId>, BTreeSet<FnKey>) {
    let mut defs = BTreeSet::new();
    let mut fns = BTreeSet::new();
    for f in mir.functions.iter().chain(mir.polls.iter()) {
        for b in &f.blocks {
            for s in &b.stmts {
                match s {
                    Statement::Assign(_, Rvalue::New { ctor: Some(c), .. }) => {
                        defs.insert(c.def);
                    }
                    Statement::Assign(_, Rvalue::FuncRef(c)) => {
                        fns.insert((c.def, c.args.clone()));
                    }
                    Statement::JsCall { callee, .. }
                    | Statement::Assign(_, Rvalue::JsCall { callee, .. }) => {
                        defs.insert(callee.def);
                    }
                    _ => {}
                }
            }
            if let Terminator::TailCall { callee, .. } = &b.terminator {
                fns.insert((callee.def, callee.args.clone()));
            }
        }
    }
    (defs, fns)
}

/// `None` unless `param` (with its local copies) is only read; otherwise the sink parameters it
/// is forwarded to, which must be flipped as well.
fn read_only_deps(
    f: &MirFunction,
    param: u32,
    by_key: &IndexMap<FnKey, usize>,
) -> Option<Vec<Slot>> {
    let mut aliases: BTreeSet<u32> = BTreeSet::from([param]);
    let mut grew = true;
    while grew {
        grew = false;
        for b in &f.blocks {
            for s in &b.stmts {
                if let Statement::Assign(
                    Place::Local(d),
                    Rvalue::Use(Operand::Copy(Place::Local(src)))
                    | Rvalue::Cast(Operand::Copy(Place::Local(src)), _, _),
                ) = s
                {
                    if aliases.contains(&src.0) && aliases.insert(d.0) {
                        grew = true;
                    }
                }
            }
        }
    }
    let is_alias =
        |op: &Operand| matches!(op, Operand::Copy(Place::Local(l)) if aliases.contains(&l.0));
    let any_alias = |ops: &[Operand]| ops.iter().any(is_alias);
    let js_alias = |target: &Operand, args: &[(Operand, TypeId)]| {
        is_alias(target) || args.iter().any(|(a, _)| is_alias(a))
    };
    let mut deps = Vec::new();
    let mut forward = |callee: &Callee, args: &[Operand]| -> bool {
        for (i, a) in args.iter().enumerate() {
            if !is_alias(a) || !callee.take_params.get(i).copied().unwrap_or(false) {
                continue;
            }
            let key = (callee.def, callee.args.clone());
            if !by_key.contains_key(&key) {
                return false;
            }
            deps.push((key, i));
        }
        true
    };
    for b in &f.blocks {
        for s in &b.stmts {
            let ok = match s {
                Statement::Assign(Place::Local(l), _) if l.0 == param => false,
                Statement::Assign(place, rv) => {
                    let stored = !matches!(place, Place::Local(_))
                        && matches!(rv, Rvalue::Use(op) | Rvalue::Cast(op, _, _) if is_alias(op));
                    !stored
                        && match rv {
                            Rvalue::Move { src, .. } => !aliases.contains(&src.0),
                            Rvalue::Call { callee, args } => forward(callee, args),
                            Rvalue::New { args, .. }
                            | Rvalue::UnionNew { args, .. }
                            | Rvalue::ArrayLit { elems: args, .. }
                            | Rvalue::Tuple { elems: args, .. }
                            | Rvalue::IndirectCall { args, .. } => !any_alias(args),
                            Rvalue::InterfaceCall { receiver, args, .. } => {
                                !is_alias(receiver) && !any_alias(args)
                            }
                            Rvalue::JsCall { target, args, .. } => !js_alias(target, args),
                            _ => true,
                        }
                }
                Statement::Call { callee, args } => forward(callee, args),
                Statement::IndirectCall { args, .. } => !any_alias(args),
                Statement::InterfaceCall { receiver, args, .. } => {
                    !is_alias(receiver) && !any_alias(args)
                }
                Statement::JsCall { target, args, .. } => !js_alias(target, args),
                Statement::Retain(op)
                | Statement::Release(op)
                | Statement::ReleaseUnique(op)
                | Statement::ForceFree(op) => !is_alias(op),
                _ => true,
            };
            if !ok {
                return None;
            }
        }
        let ok = match &b.terminator {
            Terminator::Return(Some(op)) | Terminator::AsyncComplete(Some(op)) => !is_alias(op),
            Terminator::TailCall { args, .. } => !any_alias(args),
            Terminator::Await { future, .. } => !is_alias(future),
            _ => true,
        };
        if !ok {
            return None;
        }
    }
    Some(deps)
}

enum ArgKind {
    /// The caller owns a reference that outlives the call.
    Owned,
    /// Safe only if the callee cannot drop a slot-held reference of this type.
    MaybeUnowned,
    /// The caller's own sink parameter at its last use: safe (as [`ArgKind::MaybeUnowned`]) only
    /// if that parameter is flipped as well, so neither frame owned it.
    WithCaller(Slot),
    Reject,
}

/// Per-caller facts for classifying call arguments.
struct CallerArgs {
    live_out: Vec<HashSet<u32>>,
    params: HashSet<u32>,
    /// Some definition is a fresh value (not a copy or slot load), so RC insertion makes the
    /// local an owner rather than a cursor.
    fresh_def: Vec<bool>,
}

impl CallerArgs {
    fn new(f: &MirFunction) -> Self {
        let mut fresh_def = vec![false; f.locals.len()];
        for b in &f.blocks {
            for s in &b.stmts {
                let Statement::Assign(Place::Local(d), rv) = s else {
                    continue;
                };
                fresh_def[d.0 as usize] |= !matches!(
                    rv,
                    Rvalue::Use(Operand::Copy(_) | Operand::Const(crate::Const::Null))
                        | Rvalue::Cast(Operand::Copy(_), _, _)
                        | Rvalue::UnionField { .. }
                );
            }
        }
        CallerArgs {
            live_out: super::rc::liveness::live_out(f),
            params: f.params.iter().map(|p| p.0).collect(),
            fresh_def,
        }
    }

    /// Calls `visit(callee, facts, pos, arg)` for every direct-call argument, with `facts`
    /// answering liveness *after* that call.
    fn visit(&self, f: &MirFunction, mut visit: impl FnMut(&Callee, &ArgsAt<'_>, usize, &Operand)) {
        for (bi, b) in f.blocks.iter().enumerate() {
            let mut live = self.live_out[bi].clone();
            super::rc::liveness::add_terminator_reads(&b.terminator, &mut live);
            for s in b.stmts.iter().rev() {
                if let Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args }) = s
                {
                    let at = ArgsAt {
                        facts: self,
                        live_after: &live,
                    };
                    for (pos, a) in args.iter().enumerate() {
                        visit(callee, &at, pos, a);
                    }
                }
                super::rc::liveness::transfer_stmt(s, &mut live);
            }
        }
    }
}

struct ArgsAt<'a> {
    facts: &'a CallerArgs,
    live_after: &'a HashSet<u32>,
}

impl ArgsAt<'_> {
    fn classify(
        &self,
        f: &MirFunction,
        arg: &Operand,
        caller_slot: &dyn Fn(u32) -> Option<Slot>,
    ) -> ArgKind {
        let l = match arg {
            Operand::Const(_) => return ArgKind::Owned,
            Operand::Copy(Place::Local(l)) => l.0,
            Operand::Copy(_) => return ArgKind::MaybeUnowned,
        };
        let live = self.live_after.contains(&l);
        if self.facts.params.contains(&l) {
            if !f.locals[l as usize].is_take {
                return ArgKind::MaybeUnowned;
            }
            return match (caller_slot(l), live) {
                (Some(slot), _) => ArgKind::WithCaller(slot),
                (None, true) => ArgKind::Owned,
                (None, false) => ArgKind::Reject,
            };
        }
        // An owned local passed at its last use is released right after a borrowing call (see
        // `last_use_destroy_site`), where the callee's frame would have released the sink.
        if self.facts.fresh_def[l as usize] {
            ArgKind::Owned
        } else {
            ArgKind::MaybeUnowned
        }
    }
}

fn direct_callees_mut(f: &mut MirFunction) -> impl Iterator<Item = &mut Callee> {
    f.blocks
        .iter_mut()
        .flat_map(|b| b.stmts.iter_mut())
        .filter_map(|s| match s {
            Statement::Call { callee, .. } | Statement::Assign(_, Rvalue::Call { callee, .. }) => {
                Some(callee)
            }
            _ => None,
        })
}

/// Type-level reachability over strong fields.
struct Reach<'a> {
    interner: &'a TypeInterner,
    layouts: &'a LayoutTable,
}

impl Reach<'_> {
    /// Whether a slot holding a `from` value can (transitively) own a `target` object.
    fn may_reach(&self, from: TypeId, target: TypeId) -> bool {
        if strong_children(target, self.interner, self.layouts).is_none() {
            return true;
        }
        let mut seen = BTreeSet::new();
        let mut stack = vec![from];
        while let Some(t) = stack.pop() {
            if t == target {
                return true;
            }
            if !seen.insert(t) {
                continue;
            }
            match strong_children(t, self.interner, self.layouts) {
                Some(cs) => stack.extend(cs),
                None => return true,
            }
        }
        false
    }

    /// Whether overwriting the slots in `k` may drop the last reference to a `target` object.
    fn may_drop(&self, k: &Known, target: TypeId) -> bool {
        if k.any_slot {
            return true;
        }
        let field_ty = |(t, f): (TypeId, u32)| {
            self.layouts
                .get(t)
                .and_then(|l| l.fields.get(f as usize))
                .map(|fl| fl.ty)
        };
        k.fields.iter().any(|&key| match field_ty(key) {
            Some(ft) => self.may_reach(ft, target),
            None => true,
        }) || k.slots.iter().any(|&s| self.may_reach(s, target))
    }
}
