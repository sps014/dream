//! Code-generation attributes: intrinsics, inlining hints and `@unsafe`.

use super::*;

pub const EXPORT_ATTR: &str = "export";

pub fn has_export_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, EXPORT_ATTR)
}

/// True when the declaration carries `@inline` (raised inliner size budget).
pub fn has_inline_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "inline")
}

/// True when the declaration carries `@noinline`.
pub fn has_noinline_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "noinline")
}
