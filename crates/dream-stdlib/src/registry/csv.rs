use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.csv",
    deps: &[
        "system.io",
        "system.text",
        "system.encoding",
        "system.json",
        "system.collections",
    ],
    generators: &[],
    files: &[(
        "<std>/system/csv/csv.dream",
        include_str!("../system/csv/csv.dream"),
    )],
};
