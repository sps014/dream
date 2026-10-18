//! Test, generator and attribute-declaration attributes.

use super::*;

pub const GENERATOR_ATTR: &str = "generator";
pub const GENERATOR_ONLY_ATTR: &str = "generator_only";
pub const INCREMENTAL_ATTR: &str = "incremental";
pub const ON_ATTRIBUTE_ATTR: &str = "on_attribute";
pub const USES_TOOL_ATTR: &str = "uses_tool";
pub const ON_CALL_ATTR: &str = "on_call";
pub const SYNTAX_BLOCK_ATTR: &str = "syntax_block";
pub const ATTRIBUTE_ATTR: &str = "attribute";
pub const REPEATABLE_ATTR: &str = "repeatable";
pub const TEST_ATTR: &str = "test";
pub const TIMEOUT_ATTR: &str = "timeout";

/// True when the declaration carries `@test`.
pub fn has_test_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, TEST_ATTR)
}

/// `@timeout(ms)`: `None` when absent, `Some(None)` when its argument is not a positive `int`.
pub fn test_timeout_ms(attributes: &[AttributeNode]) -> Option<Option<u64>> {
    let attr = attributes
        .iter()
        .find(|a| is_compiler_attribute(a, TIMEOUT_ATTR))?;
    Some(match attr.args.as_slice() {
        [AttributeArg::Int(t)] => t
            .text
            .parse::<u64>()
            .ok()
            .filter(|&ms| ms > 0 && ms <= i32::MAX as u64),
        _ => None,
    })
}

/// The attribute-type paths of `@generator_only(A, ...)`, or `None` when the declaration does not
/// carry it.
pub fn generator_only_paths(attributes: &[AttributeNode]) -> Option<Vec<String>> {
    let attr = attributes
        .iter()
        .find(|a| is_compiler_attribute(a, GENERATOR_ONLY_ATTR))?;
    Some(attr.args.iter().map(|arg| arg.semantic_value()).collect())
}

/// True when the declaration carries `@generator`.
pub fn has_generator_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, GENERATOR_ATTR)
}

/// True when the type declaration is an `@attribute` declaration.
pub fn has_attribute_decl_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, ATTRIBUTE_ATTR)
}
