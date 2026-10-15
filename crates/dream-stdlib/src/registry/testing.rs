use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.testing",
    deps: &["system.core", "system.primitives", "system", "system.io"],
    generators: &[],
    files: &[
        (
            "<std>/system/testing/attributes.dream",
            include_str!("../system/testing/attributes.dream"),
        ),
        (
            "<std>/system/testing/fixture.dream",
            include_str!("../system/testing/fixture.dream"),
        ),
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
