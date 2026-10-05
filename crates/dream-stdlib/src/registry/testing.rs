use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.testing",
    deps: &["system.core", "system.primitives", "system"],
    files: &[
        (
            "<std>/system/testing/assert.dream",
            include_str!("../system/testing/assert.dream"),
        ),
        (
            "<std>/system/testing/test.dream",
            include_str!("../system/testing/test.dream"),
        ),
    ],
};
