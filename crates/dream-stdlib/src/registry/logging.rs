use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.logging",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system",
        "system.io",
    ],
    files: &[
        (
            "<std>/system/logging/log_level.dream",
            include_str!("../system/logging/log_level.dream"),
        ),
        (
            "<std>/system/logging/log_record.dream",
            include_str!("../system/logging/log_record.dream"),
        ),
        (
            "<std>/system/logging/log_handler.dream",
            include_str!("../system/logging/log_handler.dream"),
        ),
        (
            "<std>/system/logging/console_handler.dream",
            include_str!("../system/logging/console_handler.dream"),
        ),
        (
            "<std>/system/logging/file_handler.dream",
            include_str!("../system/logging/file_handler.dream"),
        ),
        (
            "<std>/system/logging/logger.dream",
            include_str!("../system/logging/logger.dream"),
        ),
    ],
};
