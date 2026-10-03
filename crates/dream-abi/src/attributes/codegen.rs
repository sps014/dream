//! Code-generation attributes: intrinsics, inlining hints and `@unsafe`.

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: "intrinsic",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Marks an extern function/method as a compiler intrinsic. The string argument is the intrinsic key (e.g. `\"print\"`).",
    },
    AttributeSpec {
        name: "inline",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Raises the inliner's size budget for this function/method. A compiler hint, not a guarantee.",
    },
    AttributeSpec {
        name: "noinline",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Never inline this function/method (neither the MIR inliner nor LLVM). Keeps benchmark sinks opaque.",
    },
    AttributeSpec {
        name: "unsafe",
        // Gates manual-memory-management operations (raw `Pointer<T>`): calling an `@unsafe`
        // function/method is only permitted from another `@unsafe` function/method — checked at
        // every call site, not just here at the declaration (see
        // `FunctionTableInfo::is_unsafe`/`Analyzer::check_unsafe_call`).
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
            AttributeTarget::ExternFunction,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function/method as unsafe. Calls are only allowed from other `@unsafe` contexts.",
    },
];

/// True when the declaration carries `@inline` (raised inliner size budget).
pub fn has_inline_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "inline")
}

/// True when the declaration carries `@noinline`.
pub fn has_noinline_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "noinline")
}
