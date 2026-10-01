//! Inferred unique-graph bump region: wrap `x = f(...); … ReleaseUnique x` when `f` only allocates
//! `del`-free class instances that cannot escape the region. The runtime TLS slab then rewinds in
//! O(1) instead of walking/recycling each node.

use super::ModulePass;
use crate::{
    Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::{IndexMap, IndexSet};
use std::collections::{BTreeMap, BTreeSet};

pub struct UniqueRegion;

mod candidates;
mod rewrite;
mod safety;
mod strip;
#[cfg(test)]
mod tests;

use candidates::*;
use rewrite::*;
use safety::*;
pub use strip::strip_escaped_regions;

impl ModulePass for UniqueRegion {
    fn name(&self) -> &'static str {
        "unique-region"
    }

    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool {
        let ctor_only = ctor_only_defs(mir);
        let safe = compute_safety(mir, interner, &ctor_only);
        let mut changed = false;
        let n = mir.functions.len();
        for i in 0..n {
            if mir.functions[i].is_async {
                continue;
            }
            let sites = wrap_sites(mir, interner, i, &ctor_only, &safe);
            if sites.is_empty() {
                continue;
            }
            apply_wraps(&mut mir.functions[i], &sites);
            changed = true;
        }
        changed
    }
}
