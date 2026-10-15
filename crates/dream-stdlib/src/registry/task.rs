use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.task",
    deps: &["system.core", "system.collections"],
    generators: &[],
    files: &[
        (
            "<std>/system/task/shared_channel.dream",
            include_str!("../system/task/shared_channel.dream"),
        ),
        (
            "<std>/system/task/task_map.dream",
            include_str!("../system/task/task_map.dream"),
        ),
        (
            "<std>/system/task/channel.dream",
            include_str!("../system/task/channel.dream"),
        ),
        (
            "<std>/system/task/task_failure.dream",
            include_str!("../system/task/task_failure.dream"),
        ),
        (
            "<std>/system/task/task_scope.dream",
            include_str!("../system/task/task_scope.dream"),
        ),
        (
            "<std>/system/task/task.dream",
            include_str!("../system/task/task.dream"),
        ),
        (
            "<std>/system/task/task_pool.dream",
            include_str!("../system/task/task_pool.dream"),
        ),
    ],
};
