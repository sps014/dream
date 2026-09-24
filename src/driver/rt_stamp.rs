//! Staleness stamps for the prebuilt runtime caches (native `libdream_rt.a`, wasm32 objects).
//!
//! Those caches are shared by every toolchain that builds in the same project or user directory,
//! so "sources older than the stamp" is not enough: alternating toolchains, or installing one whose
//! sources keep older mtimes, would link another runtime's objects. The stamp instead records the
//! exact inputs (path, size, mtime), and the cache is reused only when they match.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// One line per input, in sorted path order.
pub fn fingerprint(mut inputs: Vec<PathBuf>) -> String {
    inputs.sort();
    inputs.dedup();
    inputs
        .iter()
        .map(|p| {
            let meta = std::fs::metadata(p).ok();
            let mtime = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            let len = meta.map_or(0, |m| m.len());
            format!("{} {len} {mtime}\n", p.display())
        })
        .collect()
}

/// Whether `stamp` holds exactly `fingerprint`.
pub fn matches(stamp: &std::path::Path, fingerprint: &str) -> bool {
    std::fs::read_to_string(stamp).is_ok_and(|s| s == fingerprint)
}
