use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.csv",
    deps: &[
        "system.io",
        "system.text",
        "system.encoding",
        "system.collections",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/csv/csv_error.dream",
            include_str!("../system/csv/csv_error.dream"),
        ),
        (
            "<std>/system/csv/csv_syntax.dream",
            include_str!("../system/csv/csv_syntax.dream"),
        ),
        (
            "<std>/system/csv/csv_reader.dream",
            include_str!("../system/csv/csv_reader.dream"),
        ),
        (
            "<std>/system/csv/csv_writer.dream",
            include_str!("../system/csv/csv_writer.dream"),
        ),
    ],
};
