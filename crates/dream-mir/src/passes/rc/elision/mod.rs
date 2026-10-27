//! [`RcElision`]: cancel redundant Retain/Release pairs.

mod branches;
mod chains;
mod pipeline;
mod postdom;
#[cfg(test)]
mod tests;

pub use pipeline::RcElision;

/// Module context lets surplus-token proofs cross summarized calls. The function-local
/// sweep remains useful inside the cheap simplification fixpoint.
pub(crate) fn run_module(mir: &mut crate::Mir, interner: &dream_types::TypeInterner) -> bool {
    use crate::passes::MirPass;
    if mir.profile.is_debug() {
        return false;
    }
    let effects = crate::analysis::effects::ModRefTable::after_rc(mir, interner);
    let mut changed = false;
    for f in &mut mir.functions {
        if f.is_async {
            continue;
        }
        changed |= super::held::run_function(f, interner, &mir.layouts, &effects);
        changed |= RcElision.run_with_layouts(f, interner, &mir.layouts);
    }
    changed
}
