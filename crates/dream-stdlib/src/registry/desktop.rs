use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.desktop",
    deps: &[
        "system.core",
        "system.primitives",
        "system.text",
        "system.collections",
        "system.encoding",
        "system",
    ],
    files: &[
        (
            "<std>/system/desktop/desktop_error.dream",
            include_str!("../system/desktop/desktop_error.dream"),
        ),
        (
            "<std>/system/desktop/desktop_wire.dream",
            include_str!("../system/desktop/desktop_wire.dream"),
        ),
        (
            "<std>/system/desktop/file_dialog.dream",
            include_str!("../system/desktop/file_dialog.dream"),
        ),
        (
            "<std>/system/desktop/message_dialog.dream",
            include_str!("../system/desktop/message_dialog.dream"),
        ),
        (
            "<std>/system/desktop/clipboard.dream",
            include_str!("../system/desktop/clipboard.dream"),
        ),
        (
            "<std>/system/desktop/shell.dream",
            include_str!("../system/desktop/shell.dream"),
        ),
    ],
};
