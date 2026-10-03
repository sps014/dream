//! The `file:line` a panic reports: the function's source file and the `SourceLine` marker in
//! effect at the check. HIR emission puts a marker before every statement whose line differs from
//! the previous statement in source order, so a block inherits the line its source-order
//! predecessor ended on.
//!
//! A panic inside library code (the stdlib or a dependency package) reports the program line that
//! called into the library instead: every sync library function takes the caller's location as a
//! trailing hidden `ptr`, forwards it to the library functions it calls, and falls back to its own
//! line when called through a table (where its `__abi` wrapper passes NULL).

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use super::lcx::Lcx;
use crate::{MirFunction, Statement};

/// Whether `f`'s direct symbol `name` takes the caller's location. Symbols the runtime calls by
/// name keep the signature it was compiled against.
pub(super) fn tracks_caller(l: &Lcx<'_>, f: &MirFunction, name: &str) -> bool {
    !f.is_async
        && name != "main"
        && !l.sigs.has_function(name)
        && f.file.as_deref().is_some_and(dream_stdlib::is_library_source)
}

/// The line in effect on entry to each block: the exit line of its lowest-numbered predecessor
/// (lowering numbers blocks in source order), or `None` for the entry block and blocks reached
/// only through back edges.
fn block_entry_lines(f: &MirFunction) -> Vec<Option<u32>> {
    let n = f.blocks.len();
    let mut first_pred: Vec<Option<usize>> = vec![None; n];
    for (bi, block) in f.blocks.iter().enumerate() {
        for succ in block.terminator.successors() {
            let s = succ.0 as usize;
            if bi < s && first_pred[s].is_none_or(|p| bi < p) {
                first_pred[s] = Some(bi);
            }
        }
    }
    let mut entry = vec![None; n];
    let mut exit: Vec<Option<u32>> = vec![None; n];
    for (bi, block) in f.blocks.iter().enumerate() {
        if bi != f.entry.0 as usize {
            entry[bi] = first_pred[bi].and_then(|p| exit[p]);
        }
        exit[bi] = block
            .stmts
            .iter()
            .rev()
            .find_map(|s| match s {
                Statement::SourceLine(line) => Some(*line),
                _ => None,
            })
            .or(entry[bi]);
    }
    entry
}

impl<'l, 'a> Fx<'l, 'a> {
    /// Starts tracking source lines for this body; `named` supplies the file for an async poll
    /// body, whose own MIR carries none.
    pub fn source_begin(&mut self, named: &MirFunction) {
        self.src_file = self.f.file.clone().or_else(|| named.file.clone());
        if self.src_file.is_some() {
            self.src_lines = block_entry_lines(self.f);
        }
        if self.l.tracked.contains(&self.l.user_fn(named)) {
            self.caller_loc = Some(self.w.param(self.w.param_count() - 1));
        }
    }

    pub fn tracks_caller(&self) -> bool {
        self.caller_loc.is_some()
    }

    pub fn source_block(&mut self, bi: usize) {
        self.src_line = self.src_lines.get(bi).copied().flatten();
    }

    pub fn source_line(&mut self, line: u32) {
        self.src_line = Some(line);
    }

    /// `const char*` naming the `file:line` a panic here reports, or NULL when unknown.
    pub fn panic_location(&mut self) -> V {
        let own = match (&self.src_file, self.src_line) {
            (Some(file), Some(line)) => Some(self.l.cstr(&format!("{file}:{line}"))),
            _ => None,
        };
        let loc = match (self.caller_loc.clone(), own) {
            (Some(caller), Some(own)) => {
                let untracked = self.w.icmp("eq", &caller, &Value::null());
                self.w.select(&untracked, &own, &caller)
            }
            (Some(caller), None) => caller,
            (None, Some(own)) => own,
            (None, None) => Value::zero(Ty::Ptr),
        };
        V::u(loc)
    }
}
