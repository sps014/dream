use super::*;

#[cfg(test)]
mod tests;

/// Region repair and final validation share the same graph/call/inline-value provenance proof.
pub fn strip_escaped_regions(mir: &mut Mir, interner: &TypeInterner) -> bool {
    if !mir.functions.iter().chain(&mir.polls).any(has_regions) {
        return false;
    }
    let verifier = crate::verify::RegionVerifier::new(mir, interner);
    let mut changed = false;
    for f in mir.functions.iter_mut().chain(&mut mir.polls) {
        if !has_regions(f) {
            continue;
        }
        let found = verifier.check(f, interner);
        if found.is_empty() {
            continue;
        }
        let details: Vec<_> = found
            .iter()
            .map(|v| format!("bb{}[{}]: {}", v.block, v.stmt, v.msg))
            .collect();
        if cfg!(debug_assertions) {
            crate::internal_error!(
                "escaped inferred region in {} after function passes: {}",
                f.name,
                details.join("; ")
            );
        }
        eprintln!(
            "warning: removed escaped inferred region in {} after function passes: {}",
            f.name,
            details.join("; ")
        );
        remove_regions(f);
        changed = true;
    }
    changed
}

fn has_regions(f: &MirFunction) -> bool {
    f.blocks.iter().any(|b| {
        b.stmts
            .iter()
            .any(|s| matches!(s, Statement::RegionEnter | Statement::RegionLeave))
    })
}

fn remove_regions(f: &mut MirFunction) {
    // Pairing is a CFG property, not linear block order. Removing the complete set leaves
    // no partial nested scope or unmatched marker on another path.
    for block in &mut f.blocks {
        block
            .stmts
            .retain(|s| !matches!(s, Statement::RegionEnter | Statement::RegionLeave));
    }
}

#[cfg(test)]
pub(super) fn strip_escaped_fn(f: &mut MirFunction, interner: &TypeInterner) -> bool {
    if crate::verify::RegionVerifier::standalone(interner)
        .check(f, interner)
        .is_empty()
    {
        return false;
    }
    remove_regions(f);
    true
}
