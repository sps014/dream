use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system",
    deps: &[
        "system.core",
        "system.primitives",
        "system.text",
        "system.io",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/arg_error.dream",
            include_str!("../system/arg_error.dream"),
        ),
        (
            "<std>/system/platform.dream",
            include_str!("../system/platform.dream"),
        ),
        (
            "<std>/system/os_family.dream",
            include_str!("../system/os_family.dream"),
        ),
        (
            "<std>/system/system.dream",
            include_str!("../system/system.dream"),
        ),
        (
            "<std>/system/stdio.dream",
            include_str!("../system/stdio.dream"),
        ),
        (
            "<std>/system/random.dream",
            include_str!("../system/random.dream"),
        ),
        (
            "<std>/system/console_color.dream",
            include_str!("../system/console_color.dream"),
        ),
        (
            "<std>/system/time.dream",
            include_str!("../system/time.dream"),
        ),
        (
            "<std>/system/stopwatch.dream",
            include_str!("../system/stopwatch.dream"),
        ),
        (
            "<std>/system/datetime_ymd.dream",
            include_str!("../system/datetime_ymd.dream"),
        ),
        (
            "<std>/system/timezone.dream",
            include_str!("../system/timezone.dream"),
        ),
        (
            "<std>/system/datetime.dream",
            include_str!("../system/datetime.dream"),
        ),
        (
            "<std>/system/debug.dream",
            include_str!("../system/debug.dream"),
        ),
        (
            "<std>/system/ffi.dream",
            include_str!("../system/ffi.dream"),
        ),
    ],
};
