use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.codegen",
    deps: &[
        "system.core",
        "system.primitives",
        "system.text",
        "system",
        "system.io",
        "system.collections",
        "system.json",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/codegen/codegen.dream",
            include_str!("../system/codegen/codegen.dream"),
        ),
        (
            "<std>/system/codegen/gen_model.dream",
            include_str!("../system/codegen/gen_model.dream"),
        ),
        (
            "<std>/system/codegen/gen_decls.dream",
            include_str!("../system/codegen/gen_decls.dream"),
        ),
        (
            "<std>/system/codegen/gen_decode.dream",
            include_str!("../system/codegen/gen_decode.dream"),
        ),
        (
            "<std>/system/codegen/gen_context.dream",
            include_str!("../system/codegen/gen_context.dream"),
        ),
    ],
};
