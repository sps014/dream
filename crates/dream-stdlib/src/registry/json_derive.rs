use super::StdPackage;

/// The `@json` derive generator. Registered through `system.json`'s `generators`; only its
/// generator executable compiles it, user programs never do.
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.json.derive",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.json",
        "system.codegen",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/json/derive/json_field_spec.dream",
            include_str!("../system/json/derive/json_field_spec.dream"),
        ),
        (
            "<std>/system/json/derive/json_collection_spec.dream",
            include_str!("../system/json/derive/json_collection_spec.dream"),
        ),
        (
            "<std>/system/json/derive/json_variant_spec.dream",
            include_str!("../system/json/derive/json_variant_spec.dream"),
        ),
        (
            "<std>/system/json/derive/json_type_spec.dream",
            include_str!("../system/json/derive/json_type_spec.dream"),
        ),
        (
            "<std>/system/json/derive/json_gen_output.dream",
            include_str!("../system/json/derive/json_gen_output.dream"),
        ),
        (
            "<std>/system/json/derive/json_generator.dream",
            include_str!("../system/json/derive/json_generator.dream"),
        ),
        (
            "<std>/system/json/derive/json_derive.dream",
            include_str!("../system/json/derive/json_derive.dream"),
        ),
        (
            "<std>/system/json/derive/construction.dream",
            include_str!("../system/json/derive/construction.dream"),
        ),
    ],
};
