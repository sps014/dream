//! `@runtime("…")` host field names shared by stdlib, native libdream, and JS `Dream.*`.

pub const PROCESS_ARGS: &str = "processArgs";
pub const PROCESS_EXE_PATH: &str = "processExePath";

pub const WORKER_SPAWN: &str = "workerSpawn";
pub const WORKER_DISPATCH: &str = "workerDispatch";
pub const WORKER_SHUTDOWN: &str = "workerShutdown";
pub const WORKER_TERMINATE: &str = "workerTerminate";
pub const WORKER_FAILURE: &str = "workerFailure";

/// Host fields that require imported shared linear memory (owner + `Task` thread instances).
pub const WORKER_IMPORT_FIELDS: &[&str] = &[
    WORKER_SPAWN,
    WORKER_DISPATCH,
    WORKER_SHUTDOWN,
    WORKER_TERMINATE,
    WORKER_FAILURE,
];

pub fn is_worker_host_field(field: &str) -> bool {
    WORKER_IMPORT_FIELDS.contains(&field)
}
