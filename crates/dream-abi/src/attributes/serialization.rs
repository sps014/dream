//! `@json` derive attributes.

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: "json",
        targets: &[
            AttributeTarget::Struct,
            AttributeTarget::ValueStruct,
            AttributeTarget::Union,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc:
            "Enables JSON serialize/deserialize derive for a class, struct, or discriminated union.",
    },
    AttributeSpec {
        name: "property_name",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Overrides the JSON property name for a field (used with `@json`).",
    },
    AttributeSpec {
        name: "json_ignore",
        targets: &[AttributeTarget::Field],
        args: ArgShape::None,
        repeatable: false,
        doc: "Excludes a field from JSON serialize/deserialize (used with `@json`).",
    },
];
