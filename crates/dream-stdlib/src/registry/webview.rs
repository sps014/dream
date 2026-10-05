use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.webview",
    deps: &[
        "system.core",
        "system.primitives",
        "system.text",
        "system.collections",
        "system.json",
        "system.encoding",
        "system",
    ],
    files: &[
        (
            "<std>/system/webview/webview_error.dream",
            include_str!("../system/webview/webview_error.dream"),
        ),
        (
            "<std>/system/webview/webview_wire_reader.dream",
            include_str!("../system/webview/webview_wire_reader.dream"),
        ),
        (
            "<std>/system/webview/webview.dream",
            include_str!("../system/webview/webview.dream"),
        ),
    ],
};
