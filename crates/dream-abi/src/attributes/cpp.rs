//! `@cpp` bindings.

use super::*;

const CPP: &str = "cpp";
const CPP_NAME: &str = "cpp_name";

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: CPP,
        targets: &[
            AttributeTarget::Struct,
            AttributeTarget::ValueStruct,
            AttributeTarget::ExternFunction,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String, ArgKind::String],
            min: 1,
            max: 2,
        },
        repeatable: false,
        doc: "Binds a class, `@unmanaged` struct, or free extern function to C++: `@cpp(\"header.hpp\", \"ns::Name\")`. The header resolves against the declaring package's `native/include/`; the name defaults to the Dream name. The compiler generates the `extern \"C\"` shim for native or portable WASM builds.",
    },
    AttributeSpec {
        name: CPP_NAME,
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "The C++ member (or expression) a `@cpp` extern calls, for renames and template instantiations: `@cpp_name(\"get_as<int>\")`.",
    },
];

/// The `@cpp(...)` attribute itself, for diagnostics that point at it.
pub fn cpp_attr(attributes: &[AttributeNode]) -> Option<&AttributeNode> {
    attributes.iter().find(|a| a.name.text == CPP)
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
