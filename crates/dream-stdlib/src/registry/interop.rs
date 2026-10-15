use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.interop",
    deps: &["system.core", "system"],
    generators: &[],
    files: &[(
        "<std>/system/interop/attributes.dream",
        include_str!("../system/interop/attributes.dream"),
    )],
};
