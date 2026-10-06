use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.simd",
    deps: &["system.core"],
    generators: &[],
    files: &[(
        "<std>/system/simd.dream",
        include_str!("../system/simd.dream"),
    )],
};
