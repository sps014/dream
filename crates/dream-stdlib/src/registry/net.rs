use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.net",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.encoding",
        "system.io",
        "system.json",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/net/http_types.dream",
            include_str!("../system/net/http_types.dream"),
        ),
        (
            "<std>/system/net/http_client.dream",
            include_str!("../system/net/http_client.dream"),
        ),
        (
            "<std>/system/net/url.dream",
            include_str!("../system/net/url.dream"),
        ),
    ],
};
