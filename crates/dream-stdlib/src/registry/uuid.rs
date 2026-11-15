use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.uuid",
    deps: &[
        "system.core",
        "system.primitives",
        "system.crypto",
        "system.encoding",
        "system.text",
    ],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/uuid/uuid_error.dream",
            include_str!("../system/uuid/uuid_error.dream"),
        ),
        (
            "<std>/system/uuid/uuid.dream",
            include_str!("../system/uuid/uuid.dream"),
        ),
    ],
};
