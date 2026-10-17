use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.compress",
    deps: &["system.io", "system.json", "system.encoding"],
    generators: &[],
    files: &[
        (
            "<std>/system/compress/compression_format.dream",
            include_str!("../system/compress/compression_format.dream"),
        ),
        (
            "<std>/system/compress/compression_step.dream",
            include_str!("../system/compress/compression_step.dream"),
        ),
        (
            "<std>/system/compress/compression_codec.dream",
            include_str!("../system/compress/compression_codec.dream"),
        ),
        (
            "<std>/system/compress/compression.dream",
            include_str!("../system/compress/compression.dream"),
        ),
        (
            "<std>/system/compress/compression_writer.dream",
            include_str!("../system/compress/compression_writer.dream"),
        ),
        (
            "<std>/system/compress/compression_reader.dream",
            include_str!("../system/compress/compression_reader.dream"),
        ),
    ],
};
