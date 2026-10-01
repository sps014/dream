use super::{ref_types, regions, returns, valid_cfg, Violation};
use crate::{Mir, MirFunction};
use dream_types::TypeInterner;

pub(crate) struct RegionVerifier {
    returns: returns::Returns,
    refs: ref_types::RefTypes,
}

impl RegionVerifier {
    #[cfg(test)]
    pub(crate) fn standalone(interner: &TypeInterner) -> Self {
        Self {
            returns: returns::Returns::new(),
            refs: ref_types::RefTypes::new(&dream_hir::LayoutTable::default(), interner),
        }
    }

    pub(crate) fn new(mir: &Mir, interner: &TypeInterner) -> Self {
        Self {
            returns: returns::summarize(mir, interner),
            refs: ref_types::RefTypes::new(&mir.layouts, interner),
        }
    }

    pub(crate) fn check(&self, f: &MirFunction, interner: &TypeInterner) -> Vec<Violation> {
        let mut out = Vec::new();
        if valid_cfg(f, &mut out) {
            regions::check(f, interner, &self.returns, &self.refs, &mut out);
        }
        out
    }
}
