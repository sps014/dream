//! Registry-based effects for bodiless operations. Unknown operations stay conservative.

use super::{Known, ModRef};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{TyKind, TypeId, TypeInterner};

pub(crate) fn pure_math_import(imp: &dream_hir::HImport, interner: &TypeInterner) -> bool {
    let field = if imp.field.is_empty() {
        &imp.name
    } else {
        &imp.field
    };
    let numeric = |t: TypeId| matches!(interner.kind(t), TyKind::Prim(p) if *p != dream_types::PrimTy::String);
    dream_abi::runtime_hosts::is_pure_math_env_import(&imp.module, field)
        && !imp.is_async
        && !imp.param_by_ref.iter().any(|&r| r)
        && imp.params.iter().all(|&t| numeric(t))
        && imp.ret.is_none_or(numeric)
}

/// Summary of a bodiless intrinsic's runtime helper, or `None` (opaque) if it may run user code
/// or overwrite object slots.
pub(super) fn intrinsic_summary(op: IntrinsicOp) -> Option<ModRef> {
    use IntrinsicOp as I;
    let observes = matches!(
        op,
        I::DebugFreeList
            | I::DebugHeapPtr
            | I::DebugLiveObjects
            | I::DebugTotalAllocations
            | I::DebugRefCount
            | I::DebugRuntimeCounter
    );
    let quiet = matches!(
        op,
        I::FuncboxFuncidx
            | I::FuncboxEnv
            | I::ArrayNew
            | I::StringAlloc
            | I::StringCharAtUnchecked
            | I::StringSet
            | I::StringFromUtf8
            | I::StringFromUtf8Prefix
            | I::StringFromUtf8PrefixN
            | I::StringSubstring
            | I::StringCopyUtf8
            | I::StringCompare
            | I::StringView
            | I::ToBytes
            | I::FromBytes
            | I::ArrayGetUnchecked
            | I::Regex
    ) || op.is_simd();
    // A panic aborts the process and never unwinds, so no later read can observe what its hook
    // does; opaque here would taint every bounds-checked helper's callers.
    let diverges = matches!(op, I::Panic);
    // Element stores of these may release an RC occupant; the element type is not known here.
    let elem_store = matches!(
        op,
        I::ArraySlotTake
            | I::ArraySlotSwap
            | I::ArrayRealloc
            | I::ArrayElemsCopy
            | I::ArrayElemsFill
            | I::ArraySetUnchecked
    );
    if !(observes || quiet || diverges || elem_store) {
        return None;
    }
    Some(ModRef::Known(Known {
        observes_rc: observes,
        any_slot: elem_store,
        may_destroy: elem_store,
        ..Known::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_box_reads_are_quiet_but_creation_stays_conservative() {
        for op in [IntrinsicOp::FuncboxFuncidx, IntrinsicOp::FuncboxEnv] {
            assert!(intrinsic_summary(op).is_some_and(|effects| effects.is_quiet()));
        }
        assert!(intrinsic_summary(IntrinsicOp::FuncboxNew).is_none());
    }
}
