//! `@runtime("…")` host field names shared by stdlib, native libdream, and JS `Dream.*`.

pub const GPU_TRY_INIT: &str = "gpuTryInit";

pub const WORKER_SPAWN: &str = "workerSpawn";
pub const WORKER_POST: &str = "workerPost";
pub const WORKER_RECV: &str = "workerRecv";
pub const WORKER_TERMINATE: &str = "workerTerminate";
pub const WORKER_POOL_SPAWN: &str = "workerPoolSpawn";
pub const WORKER_POOL_DISPATCH: &str = "workerPoolDispatch";

/// Host fields that require imported shared linear memory (owner + `WebWorker` instances).
pub const WORKER_IMPORT_FIELDS: &[&str] = &[
    WORKER_SPAWN,
    WORKER_POST,
    WORKER_RECV,
    WORKER_TERMINATE,
    WORKER_POOL_SPAWN,
    WORKER_POOL_DISPATCH,
];

pub fn is_worker_host_field(field: &str) -> bool {
    WORKER_IMPORT_FIELDS.contains(&field)
}
