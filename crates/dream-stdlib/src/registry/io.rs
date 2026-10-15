use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.io",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.encoding",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/io/temporary.dream",
            include_str!("../system/io/temporary.dream"),
        ),
        (
            "<std>/system/io/streams.dream",
            include_str!("../system/io/streams.dream"),
        ),
        (
            "<std>/system/io/memory_stream.dream",
            include_str!("../system/io/memory_stream.dream"),
        ),
        (
            "<std>/system/io/buffered_reader.dream",
            include_str!("../system/io/buffered_reader.dream"),
        ),
        (
            "<std>/system/io/buffered_writer.dream",
            include_str!("../system/io/buffered_writer.dream"),
        ),
        (
            "<std>/system/io/io_error.dream",
            include_str!("../system/io/io_error.dream"),
        ),
        (
            "<std>/system/io/path.dream",
            include_str!("../system/io/path.dream"),
        ),
        (
            "<std>/system/io/file_handle.dream",
            include_str!("../system/io/file_handle.dream"),
        ),
        (
            "<std>/system/io/file_stream.dream",
            include_str!("../system/io/file_stream.dream"),
        ),
        (
            "<std>/system/io/file_stats.dream",
            include_str!("../system/io/file_stats.dream"),
        ),
        (
            "<std>/system/io/file.dream",
            include_str!("../system/io/file.dream"),
        ),
    ],
};
