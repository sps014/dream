use super::cache::cached_harness_ll;
use super::diagnostics::parse_generator_output;
use super::diagnostics::JsonGenError;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

pub(super) const HARNESS_SOURCE: &str = include_str!("../json_gen_harness.dream");

pub(super) const SNAPSHOT_ENV: &str = "DREAM_JSON_GEN_SNAPSHOT";

pub(super) fn run_dream_json_generator(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    snapshot: &str,
) -> Result<String, JsonGenError> {
    // `System.env_or` reads process env, so concurrent compiles in this process must not
    // overlap `set_var`. Snapshot *files* are unique so another `dream` (LSP, bench) cannot
    // overwrite our input the way a shared `snapshot.json` did.
    static SNAPSHOT_GUARD: Mutex<()> = Mutex::new(());
    let _guard = SNAPSHOT_GUARD.lock().unwrap_or_else(|e| e.into_inner());

    let ll_path = cached_harness_ll(config).map_err(|e| JsonGenError {
        message: e,
        type_name: None,
        field_name: None,
    })?;
    let snap_path = write_unique_snapshot(&ll_path, snapshot)?;

    std::env::set_var(SNAPSHOT_ENV, snap_path.as_os_str());
    // The harness is a throwaway code generator that runs for milliseconds on one small snapshot,
    // so build time dominates end to end; it builds at `-O0`.
    let output = crate::execution::native::compile_and_capture(
        config,
        &ll_path,
        crate::driver::wasm_opt::OptLevel::O0,
    );
    std::env::remove_var(SNAPSHOT_ENV);
    let _ = std::fs::remove_file(&snap_path);

    let output = output.map_err(|e| JsonGenError {
        message: format!("@json generator: failed to run Dream harness: {e}"),
        type_name: None,
        field_name: None,
    })?;

    parse_generator_output(&output)
}

pub(super) fn write_unique_snapshot(
    module_path: &str,
    snapshot: &str,
) -> Result<std::path::PathBuf, JsonGenError> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::path::Path::new(module_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!(
        "snapshot-{}-{}-{}.json",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, snapshot.as_bytes()).map_err(|e| JsonGenError {
        message: format!("@json generator: failed to write snapshot: {e}"),
        type_name: None,
        field_name: None,
    })?;
    Ok(path)
}
