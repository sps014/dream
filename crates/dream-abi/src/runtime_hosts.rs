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

/// Standard component services implemented by the WASI adapter. Everything else is rejected
/// while the driver can still render a diagnostic, before code generation or linking.
pub fn component_host_supported(module: &str, field: &str) -> bool {
    if module.starts_with("c/") {
        return true;
    }
    if module != "Dream" {
        return false;
    }
    matches!(
        field,
        "timeNowNanos"
            | "dateNowMillis"
            | "delayMs"
            | "cryptoSecureRandomBytes"
            | "fileRead"
            | "fileWrite"
            | "fileAppend"
            | "fileExists"
            | "fileDelete"
            | "fileSize"
            | "fileReadBytes"
            | "fileWriteBytes"
            | "fileIsDir"
            | "fileStat"
            | "fileOpen"
            | "fileHandleRead"
            | "fileHandleWrite"
            | "fileHandleSeek"
            | "fileHandleTell"
            | "fileHandleSeekEnd"
            | "fileHandleLength"
            | "fileHandleClose"
            | "fileOperationStart"
            | "fileOperationDone"
            | "fileOperationResult"
            | "fileOperationBytes"
            | "fileOperationRelease"
            | "fileOperationWait"
            | "dirRemove"
            | "dirRemoveAll"
            | "dirCreate"
            | "dirCreateAll"
            | "dirList"
            | "fileCopy"
            | "fileRename"
            | "dirCurrent"
            | "dirSetCurrent"
            | "processEnvGet"
            | "processEnvSet"
            | "processEnvUnset"
            | "processEnvKeys"
            | "consoleReadLine"
            | "consoleReadKey"
            | "consoleExit"
            | "netUrlParse"
            | "netClientCreate"
            | "netClientClose"
            | "netHttpStart"
            | "netBodyRead"
            | "netBodyClose"
            | "netOperationPoll"
            | "netOperationRelease"
            | "netOperationClaim"
            | "netOperationWait"
            | "netCapabilities"
            | "netSocketStart"
            | "netSocketWait"
            | "netSocketResult"
            | "netSocketCancel"
            | "netSocketRelease"
            | "netSocketClaim"
            | "netSocketClose"
            | "netSocketControl"
    )
}
