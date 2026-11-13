use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.simd",
    deps: &["system.core"],
    core_language: true,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[(
        "<std>/system/vector.dream",
        include_str!("../system/vector.dream"),
    )],
};
