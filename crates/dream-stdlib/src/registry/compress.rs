use super::StdPackage;
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.compress",
    deps: &["system.io"],
    generators: &[],
    files: &[
        (
            "<std>/system/compress/compression_format.dream",
            include_str!("../system/compress/compression_format.dream"),
        ),
        (
            "<std>/system/compress/crc32.dream",
            include_str!("../system/compress/crc32.dream"),
        ),
        (
            "<std>/system/compress/adler32.dream",
            include_str!("../system/compress/adler32.dream"),
        ),
        (
            "<std>/system/compress/huffman_table.dream",
            include_str!("../system/compress/huffman_table.dream"),
        ),
        (
            "<std>/system/compress/huffman_codes.dream",
            include_str!("../system/compress/huffman_codes.dream"),
        ),
        (
            "<std>/system/compress/bit_writer.dream",
            include_str!("../system/compress/bit_writer.dream"),
        ),
        (
            "<std>/system/compress/inflater.dream",
            include_str!("../system/compress/inflater.dream"),
        ),
        (
            "<std>/system/compress/deflater.dream",
            include_str!("../system/compress/deflater.dream"),
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
