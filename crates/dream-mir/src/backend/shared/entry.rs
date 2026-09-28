//! How `main`'s declared return type becomes the process exit status.
//!
//! `main(): void` always exits 0, `main(): int` returns the code directly, and
//! `main(): Result<T, E>` prints `Error: <e>` on stderr and exits 1.

use super::cx::Cx;
use crate::MirFunction;
use dream_types::{PrimTy, TyKind, TypeId};

/// The `Err` line's prefix, matching Rust's `Error: {err:?}`. Interned like any other literal.
pub(crate) const ERROR_PREFIX: &str = "Error: ";

/// Module-level slot holding the exit status once `main` has finished.
pub(crate) const RC_SLOT: &str = "__dream_main_rc";

/// The status helper generated for a `Result`-returning `main`.
pub(crate) const STATUS_FN: &str = "__dream_main_status";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryExit {
    /// `main(): void` — always 0.
    Void,
    /// `main(): int` — the returned value is the exit code.
    Code,
    /// `main(): Result<T, E>` — `Err` reports on stderr and exits 1. Carries the `Result` type.
    Report(TypeId),
}

/// Classifies `main`'s return type. Sema has already rejected every other spelling, so anything
/// unrecognized here is treated as `void` rather than diagnosed.
pub(crate) fn entry_exit(main: &MirFunction, cx: &Cx<'_>) -> EntryExit {
    // `MirFunction::ret` is the *declared* type even for `async fun`, so the async stub and the
    // sync body classify identically.
    match cx.interner.kind(main.ret) {
        TyKind::Prim(PrimTy::Int) => EntryExit::Code,
        TyKind::Union(..) if cx.nunion(main.ret).is_some() => EntryExit::Report(main.ret),
        _ => EntryExit::Void,
    }
}

/// True when `main` returns a `Result`, so the module needs the `Error:` literal. Answerable from
/// the MIR alone, because string interning runs while the [`Cx`] is still being built.
pub(crate) fn entry_reports_error(mir: &crate::Mir) -> bool {
    mir.functions
        .iter()
        .any(|f| f.name == crate::abi::ENTRY_FN && mir.layouts.unions.contains_key(&f.ret))
}

/// The error payload type of a `Result`-returning `main`, for callers that must pre-register work
/// keyed on it — `to_string` protocol reachability in particular, which is computed from MIR
/// statements and would otherwise never see this hand-emitted call site.
pub(crate) fn entry_error_type(cx: &Cx<'_>) -> Option<TypeId> {
    let main = cx
        .mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)?;
    let EntryExit::Report(ty) = entry_exit(main, cx) else {
        return None;
    };
    err_variant(cx, ty).map(|(_, field_ty)| field_ty)
}

/// `(discriminant, payload type)` of the `Result`'s failure variant.
pub(crate) fn err_variant(cx: &Cx<'_>, ty: TypeId) -> Option<(usize, TypeId)> {
    let u = cx.nunion(ty)?;
    let v = u.variants.iter().find(|v| v.name == "Err")?;
    let field = v.fields.first()?;
    Some((v.discriminant as usize, field.ty))
}
