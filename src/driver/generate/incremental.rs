//! `@incremental` result cache: a generator whose executable and snapshot are unchanged replays
//! its last result instead of running. The snapshot already holds the target, options and
//! additional files, so the key is just executable key + snapshot text.

#[cfg(feature = "native")]
use super::model::GenResult;
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

pub fn result_key(exe_key: &str, snapshot_json: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(exe_key.as_bytes());
    hash.update(&[0]);
    hash.update(snapshot_json.as_bytes());
    hash.finalize().to_hex().to_string()
}

#[cfg(feature = "native")]
fn path(cache_root: &Path, key: &str) -> PathBuf {
    cache_root.join("results").join(format!("{key}.json"))
}

#[cfg(feature = "native")]
pub fn load(cache_root: &Path, key: &str) -> Option<GenResult> {
    let text = std::fs::read_to_string(path(cache_root, key)).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(feature = "native")]
/// Best-effort: a failed write only costs a rerun next time. Written via rename so a concurrent
/// reader never sees a partial file.
pub fn store(cache_root: &Path, key: &str, result: &GenResult) {
    let target = path(cache_root, key);
    let Some(dir) = target.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(text) = serde_json::to_string(result) else {
        return;
    };
    let scratch = dir.join(format!("{key}.{}.tmp", std::process::id()));
    if std::fs::write(&scratch, text).is_ok() && std::fs::rename(&scratch, &target).is_err() {
        let _ = std::fs::remove_file(&scratch);
    }
}
