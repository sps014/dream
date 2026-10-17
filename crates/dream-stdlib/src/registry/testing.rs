use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.testing",
    deps: &["system.core", "system.primitives", "system", "system.io"],
    generators: &[],
    files: &[
        (
            "<std>/system/testing/test_attribute.dream",
            include_str!("../system/testing/test_attribute.dream"),
        ),
        (
            "<std>/system/testing/timeout_attribute.dream",
            include_str!("../system/testing/timeout_attribute.dream"),
        ),
        (
            "<std>/system/testing/test_fixture.dream",
            include_str!("../system/testing/test_fixture.dream"),
        ),
        (
            "<std>/system/testing/fixtures.dream",
            include_str!("../system/testing/fixtures.dream"),
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
