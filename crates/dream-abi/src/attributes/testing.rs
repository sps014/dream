//! Test, generator and attribute-definition attributes.

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    // Source-generator framework (`system.codegen` / `driver/generate`).
    AttributeSpec {
        name: "generator",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function as a source generator entry point (`system.codegen`).",
    },
    AttributeSpec {
        name: "test",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a top-level `fun name(): void` as a unit test discovered by `dream test` / `dreamer test`.",
    },
    AttributeSpec {
        name: "syntax_block",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: true,
        doc: "Associates a generator with a syntax-block kind (string name).",
    },
    // User-defined attribute: `@attribute` on a bare top-level function; the function name is the
    // attribute name (exact casing), and its parameters are the `@name(...)` arg schema.
    AttributeSpec {
        name: "attribute",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Declares a user-defined attribute. The function name becomes the `@name` attribute.",
    },
];

/// True when the declaration carries `@test`.
pub fn has_test_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "test")
}

/// True when the declaration carries `@generator`.
pub fn has_generator_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "generator")
}
