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
    files: &[
        (
            "<std>/system/codegen/codegen.dream",
            include_str!("../system/codegen/codegen.dream"),
        ),
        (
            "<std>/system/codegen/gen_context.dream",
            include_str!("../system/codegen/gen_context.dream"),
        ),
    ],
};
