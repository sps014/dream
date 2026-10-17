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
    files: &[
        (
            "<std>/system/csv/csv_error.dream",
            include_str!("../system/csv/csv_error.dream"),
        ),
        (
            "<std>/system/csv/csv_codec.dream",
            include_str!("../system/csv/csv_codec.dream"),
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
