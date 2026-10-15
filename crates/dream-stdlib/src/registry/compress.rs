use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.compress",
    deps: &["system.io", "system.json", "system.encoding"],
    generators: &[],
    files: &[(
        "<std>/system/compress/compression.dream",
        include_str!("../system/compress/compression.dream"),
    )],
};
