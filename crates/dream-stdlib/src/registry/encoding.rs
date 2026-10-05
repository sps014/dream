use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.encoding",
    deps: &["system.core", "system.primitives", "system.text"],
    files: &[(
        "<std>/system/encoding.dream",
        include_str!("../system/encoding.dream"),
    )],
};
