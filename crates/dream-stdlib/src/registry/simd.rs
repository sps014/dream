use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.simd",
    deps: &["system.core"],
    generators: &[],
    files: &[(
        "<std>/system/vector.dream",
        include_str!("../system/vector.dream"),
    )],
};
