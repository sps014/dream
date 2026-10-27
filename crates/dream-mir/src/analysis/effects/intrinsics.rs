//! Registry-based effects for bodiless operations. Unknown operations stay conservative.

use super::{Known, ModRef};
use dream_abi::intrinsics::IntrinsicOp;
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
    // Collection owner counts only select equivalent copy/move implementations. Unlike
    // Debug.ref_count, they must not keep an otherwise dead snapshot token observable.
    let quiet = matches!(
        op,
        I::FuncboxFuncidx
            | I::CollectionOwnerCount
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
            | I::StringView
            | I::ToBytes
            | I::FromBytes
            | I::ArrayGetUnchecked
            | I::Regex
            | I::Float(_)
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

    #[test]
    fn implementation_owner_probes_do_not_expose_debug_counts() {
        assert!(intrinsic_summary(IntrinsicOp::CollectionOwnerCount).is_some_and(|s| s.is_quiet()));
        assert!(matches!(intrinsic_summary(IntrinsicOp::DebugRefCount),
            Some(ModRef::Known(k)) if k.observes_rc));
    }
}
