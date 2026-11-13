use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.encoding",
    deps: &["system.core", "system.primitives", "system.text"],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[(
        "<std>/system/encoding.dream",
        include_str!("../system/encoding.dream"),
    )],
};
