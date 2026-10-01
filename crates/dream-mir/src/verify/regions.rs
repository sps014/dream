//! Region stack balance must agree at every CFG join and survive no suspension or exit.

use super::{violation, Violation};
use crate::{MirFunction, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::VecDeque;

pub(super) fn check(f: &MirFunction, interner: &TypeInterner, out: &mut Vec<Violation>) {
    let initial_errors = out.len();
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(0usize);
    let mut pending = VecDeque::from([f.entry]);
    while let Some(bi) = pending.pop_front() {
        let block = &f.blocks[bi.0 as usize];
        let mut depth = incoming[bi.0 as usize].expect("queued reachable block");
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::RegionEnter => depth += 1,
                Statement::RegionLeave if depth == 0 => out.push(violation(
                    f,
                    bi.0 as usize,
                    si,
                    "region leave without an active region".into(),
                )),
                Statement::RegionLeave => depth -= 1,
                _ => {}
            }
        }
        if depth != 0
            && matches!(
                block.terminator,
                Terminator::Return(_)
                    | Terminator::AsyncComplete(_)
                    | Terminator::TailCall { .. }
                    | Terminator::Await { .. }
            )
        {
            out.push(violation(
                f,
                bi.0 as usize,
                block.stmts.len(),
                format!("{depth} active regions at function exit or suspension"),
            ));
        }
        for successor in block.terminator.successors() {
            match incoming[successor.0 as usize] {
                Some(previous) if previous != depth => out.push(violation(
                    f,
                    bi.0 as usize,
                    block.stmts.len(),
                    format!(
                        "region depth mismatch at bb{}: {previous} versus {depth}",
                        successor.0
                    ),
                )),
                None => {
                    incoming[successor.0 as usize] = Some(depth);
                    pending.push_back(successor);
                }
                _ => {}
            }
        }
    }
    if out.len() == initial_errors {
        super::region_values::check(f, interner, &incoming, out);
    }
}
