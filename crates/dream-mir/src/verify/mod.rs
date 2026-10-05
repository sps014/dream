//! Final MIR validation: CFG structure, token death and balanced allocation regions.
//! Enabled in debug builds or with DREAM_VERIFY_MIR=1. Violations are compiler bugs.
//!
//! Explicit local tokens are balanced immediately after RC insertion. Final MIR keeps
//! independent alias-death and region-provenance checks after optimizations erase transfers.

mod call_effects;
mod operands;
mod ownership;
mod ref_types;
mod region_graph;
mod region_values;
mod regions;
mod returns;
mod shared_tokens;
mod token_flow;
mod token_transfer;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod return_tests;

#[cfg(test)]
mod shared_token_tests;

#[cfg(test)]
mod token_tests;

#[cfg(test)]
mod region_tests;

#[cfg(test)]
mod region_rejection_tests;

use crate::{Mir, MirFunction};
use dream_types::TypeInterner;

/// One verifier finding, located by function, block, and statement index (`stmts.len()` = the
/// terminator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub func: String,
    pub block: usize,
    pub stmt: usize,
    pub msg: String,
}

pub fn verify_module(mir: &Mir, interner: &TypeInterner) -> Vec<Violation> {
    let returns = returns::summarize(mir, interner);
    let refs = ref_types::RefTypes::new(&mir.layouts, interner);
    mir.functions
        .iter()
        .chain(mir.polls.iter())
        .flat_map(|f| verify_with_returns(f, interner, &returns, &refs))
        .collect()
}

/// True in debug builds of the compiler, or when `DREAM_VERIFY_MIR=1` (lets release CI verify too).
pub fn enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("DREAM_VERIFY_MIR").is_some_and(|v| v == "1")
}

/// Panics (an ICE) listing every violation in `mir`.
pub fn assert_module(mir: &Mir, interner: &TypeInterner) {
    let found = verify_module(mir, interner);
    if found.is_empty() {
        return;
    }
    let lines: Vec<String> = found
        .iter()
        .map(|v| format!("  {} bb{}[{}]: {}", v.func, v.block, v.stmt, v.msg))
        .collect();
    crate::internal_error!("MIR verifier failed:\n{}", lines.join("\n"));
}

/// Check the explicit ownership boundary before inlining/elision erase transfers.
pub fn assert_inserted_tokens(mir: &Mir, interner: &TypeInterner) {
    token_flow::assert_module(mir, interner);
}

pub fn verify_function(f: &MirFunction, interner: &TypeInterner) -> Vec<Violation> {
    verify_with_returns(
        f,
        interner,
        &returns::Returns::new(),
        &ref_types::RefTypes::new(&dream_hir::LayoutTable::default(), interner),
    )
}

fn verify_with_returns(
    f: &MirFunction,
    interner: &TypeInterner,
    returns: &returns::Returns,
    refs: &ref_types::RefTypes,
) -> Vec<Violation> {
    let mut out = Vec::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        ownership::check_rc_types(f, interner, bi, block, &mut out);
    }
    if !valid_cfg(f, &mut out) {
        return out;
    }
    ownership::check_paths(f, &mut out);
    shared_tokens::check(f, interner, &mut out);
    regions::check(f, interner, returns, refs, &mut out);
    out
}

fn valid_cfg(f: &MirFunction, out: &mut Vec<Violation>) -> bool {
    let mut valid = true;
    if f.entry.0 as usize >= f.blocks.len() {
        valid = false;
        out.push(violation(
            f,
            f.entry.0 as usize,
            0,
            "entry block does not exist".into(),
        ));
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        for target in block.terminator.successors() {
            if target.0 as usize >= f.blocks.len() {
                valid = false;
                out.push(violation(
                    f,
                    bi,
                    block.stmts.len(),
                    format!("successor bb{} does not exist", target.0),
                ));
            }
        }
    }
    valid
}

fn violation(f: &MirFunction, block: usize, stmt: usize, msg: String) -> Violation {
    Violation {
        func: f.name.clone(),
        block,
        stmt,
        msg,
    }
}
