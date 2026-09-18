//! Statement-level borrow lifetime for last-use destroy.
//!
//! Every owned RC local last-use-destroys. A statement is [`StmtBorrow::Held`] only when it may
//! stash a raw guest pointer without a MIR `Retain` — keyed by [`dream_abi::intrinsics::holds_raw_borrow`],
//! never by `TyKind`.

use super::liveness::stmt_reads_local;
use crate::{BasicBlock, Callee, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::DefId;
use std::collections::HashSet;

/// Whether a statement's guest borrows end when it returns, or may outlive it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StmtBorrow {
    /// Borrowed pointers are not used after this statement without a later MIR use/`Retain`.
    Ends,
    /// May keep a raw guest pointer; last-use destroy waits for a later safe point.
    Held,
}

/// `DefId`s that may stash a raw guest pointer: [`holds_raw_borrow`] keys and `@async_host` imports.
/// Last-use destroy is delayed past the statement; in an `Await` block, `end_release` of those
/// arguments waits until the resume block so the host can keep the pointer until completion.
pub(crate) fn held_defs(
    intrinsics: &[(DefId, String)],
    imports: &[dream_hir::HImport],
) -> HashSet<DefId> {
    let mut out: HashSet<DefId> = intrinsics
        .iter()
        .filter(|(_, key)| dream_abi::intrinsics::holds_raw_borrow(key))
        .map(|(def, _)| *def)
        .collect();
    for imp in imports {
        if imp.async_host {
            out.insert(imp.def);
        }
    }
    out
}

/// Borrowed arguments of the call that produced `Await.future` in this block. The callee (or host)
/// may still read them until that await completes, so last-use destroy waits for the resume block.
pub(crate) fn call_args_kept_across_await(block: &BasicBlock, nloc: usize) -> HashSet<u32> {
    let Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        ..
    } = &block.terminator
    else {
        return HashSet::new();
    };
    let mut out = HashSet::new();
    for stmt in &block.stmts {
        if !assigns_future_call(stmt, fut.0) {
            continue;
        }
        let Some((take_params, args)) = super::tokens::sink_call_args(stmt) else {
            for local in 0..nloc as u32 {
                if stmt_reads_local(stmt, local) {
                    out.insert(local);
                }
            }
            continue;
        };
        for (i, arg) in args.iter().enumerate() {
            if take_params.get(i).copied().unwrap_or(false) {
                continue;
            }
            if let Operand::Copy(Place::Local(l)) = arg {
                out.insert(l.0);
            }
        }
    }
    out
}

fn assigns_future_call(stmt: &Statement, dest: u32) -> bool {
    match stmt {
        Statement::Assign(Place::Local(d), rv) if d.0 == dest => matches!(
            rv,
            Rvalue::Call { .. }
                | Rvalue::JsCall { .. }
                | Rvalue::IndirectCall { .. }
                | Rvalue::InterfaceCall { .. }
        ),
        _ => false,
    }
}

/// Last-use `Release` may be inserted immediately after `stmt`.
pub(crate) fn may_die_after(stmt: &Statement, holds: &HashSet<DefId>) -> bool {
    stmt_borrow(stmt, holds) == StmtBorrow::Ends
}

pub(crate) fn stmt_borrow(stmt: &Statement, holds: &HashSet<DefId>) -> StmtBorrow {
    match stmt {
        Statement::Assign(_, rv) => rvalue_borrow(rv, holds),
        Statement::Call { callee, .. } | Statement::JsCall { callee, .. } => {
            callee_borrow(callee, holds)
        }
        Statement::IndirectCall { .. } | Statement::InterfaceCall { .. } => StmtBorrow::Ends,
        Statement::Retain(_)
        | Statement::Release(_)
        | Statement::ReleaseUnique(_)
        | Statement::Panic(_)
        | Statement::Print { .. }
        | Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::ArrayElemsCopy { .. }
        | Statement::ArrayElemsFill { .. }
        | Statement::ForceFree(_)
        | Statement::LockAcquire(_)
        | Statement::LockRelease(_)
        | Statement::DeferEnter
        | Statement::DeferLeave(_)
        | Statement::RegionEnter
        | Statement::RegionLeave
        | Statement::SimdV128 { .. }
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => StmtBorrow::Ends,
    }
}

fn rvalue_borrow(rv: &Rvalue, holds: &HashSet<DefId>) -> StmtBorrow {
    match rv {
        Rvalue::Call { callee, .. } | Rvalue::JsCall { callee, .. } => callee_borrow(callee, holds),
        Rvalue::Use(_)
        | Rvalue::Move { .. }
        | Rvalue::Select { .. }
        | Rvalue::Binary(_, _, _)
        | Rvalue::Unary(_, _)
        | Rvalue::StrLen(_)
        | Rvalue::StrByteSize(_)
        | Rvalue::CharAt(_, _, _)
        | Rvalue::ByteAt(_, _, _)
        | Rvalue::ArrayNew { .. }
        | Rvalue::HashCode(_)
        | Rvalue::ToString(_)
        | Rvalue::Concat(_)
        | Rvalue::ConcatInt { .. }
        | Rvalue::EnumName { .. }
        | Rvalue::IndirectCall { .. }
        | Rvalue::InterfaceCall { .. }
        | Rvalue::FuncRef(_)
        | Rvalue::New { .. }
        | Rvalue::Tuple { .. }
        | Rvalue::UnionNew { .. }
        | Rvalue::ArrayLit { .. }
        | Rvalue::ArrayLen(_)
        | Rvalue::ToBytes { .. }
        | Rvalue::FromBytes { .. }
        | Rvalue::ArrayRealloc { .. }
        | Rvalue::Cast(_, _, _)
        | Rvalue::Discriminant { .. }
        | Rvalue::UnionField { .. }
        | Rvalue::IsType(_, _)
        | Rvalue::TypeName(_) => StmtBorrow::Ends,
    }
}

fn callee_borrow(callee: &Callee, holds: &HashSet<DefId>) -> StmtBorrow {
    if holds.contains(&callee.def) {
        StmtBorrow::Held
    } else {
        StmtBorrow::Ends
    }
}
