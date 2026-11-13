//! Registry-based effects for bodiless operations. Unknown operations stay conservative.

use super::{Known, ModRef};
use dream_abi::intrinsics::IntrinsicOp;

pub(super) fn host_summary(import: &dream_hir::HImport) -> Option<ModRef> {
    // A quiet host reads system state without accessing Dream storage or invoking callbacks.
    // This ownership fact does not imply purity or permit hoisting the call.
    (import.module == crate::abi::HOST_MODULE
        && (import.quiet || crate::runtime::modules::ownership_quiet_host(&import.field)))
    .then(ModRef::default)
}
/// Summary of a bodiless intrinsic's runtime helper, or `None` (opaque) if it may run user code
/// or overwrite object slots.
pub(super) fn intrinsic_summary(op: IntrinsicOp) -> Option<ModRef> {
    use IntrinsicOp as I;
    let observes_heap = matches!(
        op,
        I::DebugFreeList | I::DebugHeapPtr | I::DebugLiveObjects | I::DebugRuntimeCounter
    );
    let observes = op == I::DebugRefCount;
    // Collection owner counts only select equivalent copy/move implementations. Unlike
    // Debug.ref_count, they must not keep an otherwise dead snapshot token observable.
    let quiet = matches!(
        op,
        I::FuncboxFuncidx
            | I::DebugTotalAllocations
            | I::FuncboxNew
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
    if !(observes || observes_heap || quiet || diverges || elem_store) {
        return None;
    }
    Some(ModRef::Known(Known {
        observes_rc: observes,
        observes_heap,
        any_slot: elem_store,
        may_destroy: elem_store,
        ..Known::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_host_imports_are_ownership_quiet_and_other_imports_remain_unknown() {
        let import = |module: &str, field: &str, quiet: bool| dream_hir::HImport {
            def: dream_types::DefId::root(0),
            name: field.to_string(),
            module: module.to_string(),
            field: field.to_string(),
            params: vec![],
            param_by_ref: vec![],
            ret: None,
            is_async: false,
            async_host: false,
            host_capability: None,
            component: false,
            quiet,
            guest_exports: vec![],
            c_wide_strings: false,
            c_params: vec![],
            c_ret: dream_hir::CShape::Void,
        };
        let host = crate::abi::HOST_MODULE;
        assert!(host_summary(&import(host, "timeNowNanos", true)).is_some_and(|s| s.is_quiet()));
        assert!(host_summary(&import("foreign", "timeNowNanos", true)).is_none());
        assert!(host_summary(&import(host, "unknown", false)).is_none());
        assert!(host_summary(&import(host, "regex_test", false)).is_some_and(|s| s.is_quiet()));
    }

    #[test]
    fn function_box_creation_and_reads_do_not_invoke_the_callback() {
        for op in [
            IntrinsicOp::FuncboxNew,
            IntrinsicOp::FuncboxFuncidx,
            IntrinsicOp::FuncboxEnv,
        ] {
            assert!(intrinsic_summary(op).is_some_and(|effects| effects.is_quiet()));
        }
    }

    #[test]
    fn implementation_owner_probes_do_not_expose_debug_counts() {
        assert!(intrinsic_summary(IntrinsicOp::CollectionOwnerCount).is_some_and(|s| s.is_quiet()));
        assert!(matches!(intrinsic_summary(IntrinsicOp::DebugRefCount),
            Some(ModRef::Known(k)) if k.observes_rc));
    }

    #[test]
    fn scheduler_control_keeps_callbacks_and_cleanup_opaque() {
        for key in ["dream_start", "dream_cancel"] {
            let op = IntrinsicOp::from_key(key).expect("registered scheduler operation");
            assert_eq!(op, IntrinsicOp::CancellationRuntime);
            assert!(intrinsic_summary(op).is_none());
        }
    }
}
