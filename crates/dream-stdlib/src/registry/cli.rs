use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.cli",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.json",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/cli/parsed_arguments.dream",
            include_str!("../system/cli/parsed_arguments.dream"),
        ),
        (
            "<std>/system/cli/command.dream",
            include_str!("../system/cli/command.dream"),
        ),
    ],
};
