//! Gives address-taken functions a caller-neutral (+0) parameter ABI.
//!
//! A `fun` value is called through [`Statement::IndirectCall`], which knows only the interned
//! `fun(...)` shape — and [`dream_types::TyKind::Func`] carries no per-parameter ownership. So an
//! indirect call cannot tell a `take` parameter from a `borrow` one and would have to retain every
//! argument, which leaks whenever the callee is `borrow` (it never releases) and is exactly why
//! `borrow` used to be unusable on anything reachable through a funcbox.
//!
//! Wrapping each target in a retaining thunk would not pay: most funcboxes are lambdas whose
//! parameters are `take`, so nearly every callback would grow a thunk and the retain would only
//! move. Instead an address-taken function switches its reference parameters to borrowed, and every
//! call site — direct and indirect alike — then passes at +0. The caller keeps ownership across the
//! call, which it already has to, so nothing needs to retain on the callee's behalf.
//!
//! That leaves the ABI uniform for a target reachable both ways, which matters because it is also
//! how the runtime enters a funcbox: `dream_worker_invoke_raw` reaches a worker body through the
//! same function table and so releases the wire string it passes.
//!
//! Runs before [`crate::passes::RcInsertion`], which then sees the rewritten `take_params` and skips
//! the call-site retains on its own.

use crate::{Callee, Mir, MirFunction, Rvalue, Statement};
use dream_types::TypeInterner;
use std::collections::HashSet;

type FnKey = (dream_types::DefId, Vec<dream_types::TypeId>);

pub struct FuncboxAbi;

impl super::ModulePass for FuncboxAbi {
    fn name(&self) -> &'static str {
        "funcbox-abi"
    }

    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool {
        let taken = address_taken(mir);
        if taken.is_empty() {
            return false;
        }
        for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
            if taken.contains(&(f.def, f.instance.clone())) {
                borrow_params(f, interner);
            }
            for callee in callees_mut(f) {
                if taken.contains(&(callee.def, callee.args.clone())) {
                    callee.take_params = vec![false; callee.take_params.len()];
                }
            }
        }
        true
    }
}

/// Every function reachable other than by a call that names it: as a `fun` value, or through an
/// interface vtable slot. Only these switch to +0; a function that is never address-taken keeps the
/// caller-retains ABI, where a caller handing over a dying value nulls its slot instead of retaining
/// at all — strictly cheaper, and still available because every such call site is known.
fn address_taken(mir: &Mir) -> HashSet<FnKey> {
    let mut out = HashSet::new();
    for f in mir.functions.iter().chain(mir.polls.iter()) {
        for b in &f.blocks {
            for s in &b.stmts {
                if let Statement::Assign(_, Rvalue::FuncRef(c)) = s {
                    out.insert((c.def, c.args.clone()));
                }
            }
        }
    }
    // An interface method is dispatched through an itable slot, so its call sites are no better
    // informed about parameter ownership than a funcbox's. The table records concrete symbols.
    let slots: HashSet<&str> = mir
        .interfaces
        .impls
        .iter()
        .flat_map(|i| i.entries.iter())
        .flat_map(|(_, syms)| syms.iter().map(String::as_str))
        .collect();
    if !slots.is_empty() {
        for f in mir.functions.iter().chain(mir.polls.iter()) {
            if slots.contains(crate::backend::shared::func_symbol(f).as_str())
                || slots.contains(f.name.as_str())
            {
                out.insert((f.def, f.instance.clone()));
            }
        }
    }
    out
}

/// Switches the function's reference parameters to the borrowed mode. `RcInsertion` then skips the
/// scope-exit releases that paired with the caller's now-absent retain, and any store of a parameter
/// into a container retains at the store the same way it already does for a declared `borrow`.
fn borrow_params(f: &mut MirFunction, interner: &TypeInterner) {
    for p in &f.params {
        let d = &mut f.locals[p.0 as usize];
        if d.is_take && interner.is_rc_tracked(d.ty) {
            d.is_take = false;
        }
    }
}

fn callees_mut(f: &mut MirFunction) -> impl Iterator<Item = &mut Callee> {
    f.blocks
        .iter_mut()
        .flat_map(|b| b.stmts.iter_mut())
        .filter_map(|s| match s {
            Statement::Call { callee, .. }
            | Statement::Assign(_, Rvalue::Call { callee, .. })
            | Statement::Assign(_, Rvalue::FuncRef(callee)) => Some(callee),
            _ => None,
        })
}
