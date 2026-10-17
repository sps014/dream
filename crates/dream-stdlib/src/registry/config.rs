use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.config",
    deps: &[
        "system",
        "system.io",
        "system.cli",
        "system.json",
        "system.collections",
        "system.encoding",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/config/config_error.dream",
            include_str!("../system/config/config_error.dream"),
        ),
        (
            "<std>/system/config/config.dream",
            include_str!("../system/config/config.dream"),
        ),
        (
            "<std>/system/config/config_builder.dream",
            include_str!("../system/config/config_builder.dream"),
        ),
    ],
};
