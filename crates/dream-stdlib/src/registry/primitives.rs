use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.primitives",
    deps: &["system.core"],
    generators: &[],
    files: &[
        (
            "<std>/system/primitives/int.dream",
            include_str!("../system/primitives/int.dream"),
        ),
        (
            "<std>/system/primitives/long.dream",
            include_str!("../system/primitives/long.dream"),
        ),
        (
            "<std>/system/primitives/uint.dream",
            include_str!("../system/primitives/uint.dream"),
        ),
        (
            "<std>/system/primitives/ulong.dream",
            include_str!("../system/primitives/ulong.dream"),
        ),
        (
            "<std>/system/primitives/isize.dream",
            include_str!("../system/primitives/isize.dream"),
        ),
        (
            "<std>/system/primitives/usize.dream",
            include_str!("../system/primitives/usize.dream"),
        ),
        (
            "<std>/system/primitives/byte.dream",
            include_str!("../system/primitives/byte.dream"),
        ),
        (
            "<std>/system/primitives/char.dream",
            include_str!("../system/primitives/char.dream"),
        ),
        (
            "<std>/system/primitives/bool.dream",
            include_str!("../system/primitives/bool.dream"),
        ),
        (
            "<std>/system/primitives/float.dream",
            include_str!("../system/primitives/float.dream"),
        ),
        (
            "<std>/system/primitives/double.dream",
            include_str!("../system/primitives/double.dream"),
        ),
    ],
};
