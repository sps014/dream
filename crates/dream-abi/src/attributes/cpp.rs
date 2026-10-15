//! `@cpp` bindings.

use super::*;

const CPP: &str = "cpp";
const CPP_NAME: &str = "cpp_name";

/// The `@cpp(...)` attribute itself, for diagnostics that point at it.
pub fn cpp_attr(attributes: &[AttributeNode]) -> Option<&AttributeNode> {
    attributes.iter().find(|a| is_compiler_attribute(a, CPP))
}

/// The header of `@cpp("header", ...)`.
pub fn cpp_header(attributes: &[AttributeNode]) -> Option<&str> {
    named_attr_string_arg(attributes, CPP, 0)
}

/// The C++ name of `@cpp("header", "ns::Name")`, when given.
pub fn cpp_target_name(attributes: &[AttributeNode]) -> Option<&str> {
    named_attr_string_arg(attributes, CPP, 1)
}

/// The C++ member or expression of `@cpp_name("expr")`.
pub fn cpp_member_name(attributes: &[AttributeNode]) -> Option<&str> {
    named_attr_string_arg(attributes, CPP_NAME, 0)
}
