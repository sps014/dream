//! Test, generator and attribute-declaration attributes.

use super::*;

/// Variadic list of declaration paths (`@on_attribute(json, json_ignore)`).
const PATHS: ArgShape = ArgShape::Args {
    kinds: &[ArgKind::Enum],
    min: 1,
    max: usize::MAX,
};

pub const GENERATOR_ATTR: &str = "generator";
pub const INCREMENTAL_ATTR: &str = "incremental";
pub const ON_ATTRIBUTE_ATTR: &str = "on_attribute";
pub const ON_CALL_ATTR: &str = "on_call";
pub const SYNTAX_BLOCK_ATTR: &str = "syntax_block";
pub const ATTRIBUTE_ATTR: &str = "attribute";
pub const REPEATABLE_ATTR: &str = "repeatable";
pub const TEST_ATTR: &str = "test";
pub const TIMEOUT_ATTR: &str = "timeout";

pub(super) const SPECS: &[AttributeSpec] = &[
    // Source-generator framework (`system.codegen` / `driver/generate`).
    AttributeSpec {
        name: GENERATOR_ATTR,
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks `fun name(ctx: GenContext)` as a source generator (`system.codegen`). The function name is the generator name.",
    },
    AttributeSpec {
        name: INCREMENTAL_ATTR,
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Declares a generator pure over its snapshot, so the compiler may replay a cached result instead of running it.",
    },
    AttributeSpec {
        name: ON_ATTRIBUTE_ATTR,
        targets: &[AttributeTarget::Function],
        args: PATHS,
        repeatable: true,
        doc: "Runs the generator when a declaration carries one of the named `@attribute` types.",
    },
    AttributeSpec {
        name: ON_CALL_ATTR,
        targets: &[AttributeTarget::Function],
        args: PATHS,
        repeatable: true,
        doc: "Runs the generator for call sites of the named functions or methods (`Type.method`).",
    },
    AttributeSpec {
        name: SYNTAX_BLOCK_ATTR,
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Makes the generator expand `name { ... }` syntax blocks, where `name` is the generator's own name.",
    },
    AttributeSpec {
        name: TEST_ATTR,
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a top-level `fun name(): void` as a unit test discovered by `dream test` / `dreamer test`.",
    },
    AttributeSpec {
        name: TIMEOUT_ATTR,
        targets: &[AttributeTarget::Function],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Fails a `@test` function that runs longer than this many milliseconds. Tests have no timeout unless they declare one.",
    },
    AttributeSpec {
        name: ATTRIBUTE_ATTR,
        targets: &[AttributeTarget::Struct, AttributeTarget::ValueStruct],
        args: ArgShape::Args {
            kinds: &[ArgKind::Enum],
            min: 0,
            max: usize::MAX,
        },
        repeatable: false,
        doc: "Declares a `@name(...)` attribute type. Arguments are `AttributeTarget.*` placements (none = anywhere); fields are the argument schema.",
    },
    AttributeSpec {
        name: REPEATABLE_ATTR,
        targets: &[AttributeTarget::Struct, AttributeTarget::ValueStruct],
        args: ArgShape::None,
        repeatable: false,
        doc: "Allows an `@attribute` type to appear more than once on the same declaration.",
    },
];

/// True when the declaration carries `@test`.
pub fn has_test_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, TEST_ATTR)
}

/// `@timeout(ms)`: `None` when absent, `Some(None)` when its argument is not a positive `int`.
pub fn test_timeout_ms(attributes: &[AttributeNode]) -> Option<Option<u64>> {
    let attr = attributes.iter().find(|a| a.name.text == TIMEOUT_ATTR)?;
    Some(match attr.args.as_slice() {
        [AttributeArg::Int(t)] => t
            .text
            .parse::<u64>()
            .ok()
            .filter(|&ms| ms > 0 && ms <= i32::MAX as u64),
        _ => None,
    })
}

/// True when the declaration carries `@generator`.
pub fn has_generator_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, GENERATOR_ATTR)
}

/// True when the type declaration is an `@attribute` declaration.
pub fn has_attribute_decl_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, ATTRIBUTE_ATTR)
}
