use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.process",
    deps: &[
        "system.core",
        "system.primitives",
        "system.text",
        "system.encoding",
        "system.collections",
        "system.io",
        "system",
    ],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/process/process_options.dream",
            include_str!("../system/process/process_options.dream"),
        ),
        (
            "<std>/system/process/process_call.dream",
            include_str!("../system/process/process_call.dream"),
        ),
        (
            "<std>/system/process/process_reader.dream",
            include_str!("../system/process/process_reader.dream"),
        ),
        (
            "<std>/system/process/process_writer.dream",
            include_str!("../system/process/process_writer.dream"),
        ),
        (
            "<std>/system/process/process_error.dream",
            include_str!("../system/process/process_error.dream"),
        ),
        (
            "<std>/system/process/process_output.dream",
            include_str!("../system/process/process_output.dream"),
        ),
        (
            "<std>/system/process/process_run.dream",
            include_str!("../system/process/process_run.dream"),
        ),
        (
            "<std>/system/process/child_process.dream",
            include_str!("../system/process/child_process.dream"),
        ),
        (
            "<std>/system/process/process.dream",
            include_str!("../system/process/process.dream"),
        ),
    ],
};
