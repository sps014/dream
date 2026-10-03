//! The `file:line` a panic reports: the function's source file and the `SourceLine` marker in
//! effect at the check. HIR emission puts a marker before every statement whose line differs from
//! the previous statement in source order, so a block inherits the line its source-order
//! predecessor ended on.

use super::fx::{Fx, V};
use super::ir::Value;
use crate::{MirFunction, Statement};

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
    }

    pub fn source_block(&mut self, bi: usize) {
        self.src_line = self.src_lines.get(bi).copied().flatten();
    }

    pub fn source_line(&mut self, line: u32) {
        self.src_line = Some(line);
    }

    /// `const char*` naming the current `file:line` for `dream_panic_at`, or NULL when unknown.
    pub fn panic_location(&mut self) -> V {
        match (&self.src_file, self.src_line) {
            (Some(file), Some(line)) => {
                let loc = format!("{file}:{line}");
                V::u(self.l.cstr(&loc))
            }
            _ => V::u(Value::null()),
        }
    }
}
