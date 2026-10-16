//! `@runtime("…")` host field names shared by stdlib, native libdream, and JS `Dream.*`.

pub const PROCESS_ARGS: &str = "processArgs";
pub const PROCESS_EXE_PATH: &str = "processExePath";

pub const WORKER_SPAWN: &str = "workerSpawn";
pub const WORKER_DISPATCH: &str = "workerDispatch";
pub const WORKER_SHUTDOWN: &str = "workerShutdown";
pub const WORKER_TERMINATE: &str = "workerTerminate";

/// Host fields that require imported shared linear memory (owner + `Task` thread instances).
pub const WORKER_IMPORT_FIELDS: &[&str] = &[
    WORKER_SPAWN,
    WORKER_DISPATCH,
    WORKER_SHUTDOWN,
    WORKER_TERMINATE,
];

pub fn is_worker_host_field(field: &str) -> bool {
    WORKER_IMPORT_FIELDS.contains(&field)
}

/// `env` math imports: pure functions of their numeric arguments on every host (libm natively,
/// `Math.*` in JS), so they read and write no Dream heap state.
pub const PURE_MATH_ENV_FIELDS: &[&str] = &[
    "abs", "floor", "ceil", "round", "sqrt", "pow", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "log", "log10", "exp", "hypot",
];

pub fn is_pure_math_env_import(module: &str, field: &str) -> bool {
    module == "env" && PURE_MATH_ENV_FIELDS.contains(&field)
}
