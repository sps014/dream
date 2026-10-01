use super::super::{
    liveness::stmt_reads_local,
    tokens::{is_owned_local, rc_op_on_local, release_and_null},
};
use crate::{MirFunction, Operand, Place, Terminator};
use dream_types::TypeInterner;
fn resume_uses_owned(block: &crate::BasicBlock, local: u32) -> bool {
    for stmt in &block.stmts {
        if rc_op_on_local(stmt, local) {
            continue;
        }
        if stmt_reads_local(stmt, local) {
            return true;
        }
    }
    match &block.terminator {
        Terminator::Await {
            future: Operand::Copy(Place::Local(f)),
            ..
        } if f.0 == local => true,
        Terminator::Return(Some(Operand::Copy(Place::Local(f))))
        | Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(f))))
            if f.0 == local =>
        {
            true
        }
        _ => false,
    }
}

pub(super) fn insert_await_resume_releases(
    func: &mut MirFunction,
    interner: &TypeInterner,
    changed: &mut bool,
) {
    let is_owned = |l: u32| is_owned_local(func, interner, l);
    let mut resume_releases: Vec<(usize, u32)> = Vec::new();
    for block in &func.blocks {
        if let Terminator::Await {
            future,
            dest,
            resume,
        } = &block.terminator
        {
            let Operand::Copy(Place::Local(l)) = future else {
                continue;
            };
            if dest == &Some(*l) {
                continue;
            }
            if !is_owned(l.0) {
                continue;
            }
            // Token analysis drops the future at Await. Post-insertion liveness still
            // treats it as live into resume when the next loop body's drop_previous
            // reads it; skipping here leaks the last iteration's future.
            if resume_uses_owned(&func.blocks[resume.0 as usize], l.0) {
                continue;
            }
            resume_releases.push((resume.0 as usize, l.0));
        }
    }
    for (ri, local) in resume_releases {
        let already = func.blocks[ri]
            .stmts
            .iter()
            .any(|s| rc_op_on_local(s, local));
        if already {
            continue;
        }
        let mut stmts = Vec::with_capacity(func.blocks[ri].stmts.len() + 2);
        stmts.extend(release_and_null(local));
        stmts.append(&mut func.blocks[ri].stmts);
        func.blocks[ri].stmts = stmts;
        *changed = true;
    }
}
