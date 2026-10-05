use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.task",
    deps: &["system.core"],
    files: &[
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
