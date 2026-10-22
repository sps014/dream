use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.cli",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/cli/cli_error.dream",
            include_str!("../system/cli/cli_error.dream"),
        ),
        (
            "<std>/system/cli/cli_argument_kind.dream",
            include_str!("../system/cli/cli_argument_kind.dream"),
        ),
        (
            "<std>/system/cli/cli_argument.dream",
            include_str!("../system/cli/cli_argument.dream"),
        ),
        (
            "<std>/system/cli/cli_value.dream",
            include_str!("../system/cli/cli_value.dream"),
        ),
        (
            "<std>/system/cli/cli_validation.dream",
            include_str!("../system/cli/cli_validation.dream"),
        ),
        (
            "<std>/system/cli/cli_help.dream",
            include_str!("../system/cli/cli_help.dream"),
        ),
        (
            "<std>/system/cli/cli_parser.dream",
            include_str!("../system/cli/cli_parser.dream"),
        ),
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
