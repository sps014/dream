use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.app",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/app/app_phase.dream",
            include_str!("../system/app/app_phase.dream"),
        ),
        (
            "<std>/system/app/shutdown_reason.dream",
            include_str!("../system/app/shutdown_reason.dream"),
        ),
        (
            "<std>/system/app/power_source.dream",
            include_str!("../system/app/power_source.dream"),
        ),
        (
            "<std>/system/app/system_theme.dream",
            include_str!("../system/app/system_theme.dream"),
        ),
        (
            "<std>/system/app/shutdown_request.dream",
            include_str!("../system/app/shutdown_request.dream"),
        ),
        (
            "<std>/system/app/app_event.dream",
            include_str!("../system/app/app_event.dream"),
        ),
        (
            "<std>/system/app/app_error.dream",
            include_str!("../system/app/app_error.dream"),
        ),
        (
            "<std>/system/app/app_host.dream",
            include_str!("../system/app/app_host.dream"),
        ),
        (
            "<std>/system/app/hook_list.dream",
            include_str!("../system/app/hook_list.dream"),
        ),
        (
            "<std>/system/app/subscription.dream",
            include_str!("../system/app/subscription.dream"),
        ),
        (
            "<std>/system/app/start_outcome.dream",
            include_str!("../system/app/start_outcome.dream"),
        ),
        (
            "<std>/system/app/app_state.dream",
            include_str!("../system/app/app_state.dream"),
        ),
        (
            "<std>/system/app/app.dream",
            include_str!("../system/app/app.dream"),
        ),
    ],
};
