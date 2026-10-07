//! Staleness stamps for the prebuilt runtime caches (native `libdream_rt.a`, wasm32 objects).
//!
//! Those caches are shared by every toolchain that builds in the same project or user directory,
//! so "sources older than the stamp" is not enough: alternating toolchains, or installing one whose
//! sources keep older mtimes, would link another runtime's objects. The stamp instead records the
//! exact input identities (path, size, timestamps and filesystem identity), and the cache is reused only when they match.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// Every file below `root`, recursively (unordered; [`fingerprint`] sorts).
pub fn files_under(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(_) => files.push(path),
                Err(_) => {}
            }
        }
    }
    files
}

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
            let len = meta.as_ref().map_or(0, |m| m.len());
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::MetadataExt;
                meta.as_ref().map_or_else(String::new, |m| {
                    format!("{}:{}:{}:{}", m.dev(), m.ino(), m.ctime(), m.ctime_nsec())
                })
            };
            #[cfg(not(unix))]
            let identity = meta
                .as_ref()
                .and_then(|m| m.created().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or_else(String::new, |d| d.as_nanos().to_string());
            format!("{} {len} {mtime} {identity}\n", p.display())
        })
        .collect()
}

/// Whether `stamp` holds exactly `fingerprint`.
pub fn matches(stamp: &std::path::Path, fingerprint: &str) -> bool {
    std::fs::read_to_string(stamp).is_ok_and(|s| s == fingerprint)
}

/// Streams contents so equal mtimes cannot hide a changed source or transitive header.
pub fn content_fingerprint(mut inputs: Vec<PathBuf>) -> String {
    inputs.sort();
    inputs.dedup();
    inputs
        .iter()
        .map(|path| {
            let digest = tool_identity(path).unwrap_or_else(|| "unreadable".into());
            format!("{} {digest}\n", path.display())
        })
        .collect()
}
pub fn content_hash(path: &std::path::Path) -> Option<blake3::Hash> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0; 16384];
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Some(hash.finalize())
}

/// Unix change timestamps let one process reuse verified contents, including shared headers.
pub fn tool_identity(path: &std::path::Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        type ToolCache = std::collections::BTreeMap<PathBuf, (String, String)>;
        static CACHE: std::sync::OnceLock<std::sync::Mutex<ToolCache>> = std::sync::OnceLock::new();
        let stamp = |path: &std::path::Path| -> Option<String> {
            let m = std::fs::metadata(path).ok()?;
            Some(format!(
                "{}:{}:{}:{}:{}:{}:{}",
                m.dev(),
                m.ino(),
                m.len(),
                m.mtime(),
                m.mtime_nsec(),
                m.ctime(),
                m.ctime_nsec()
            ))
        };
        let before = stamp(path)?;
        let cache = CACHE.get_or_init(Default::default);
        if let Some((recorded, identity)) = cache.lock().ok()?.get(path)
            && *recorded == before
        {
            return Some(identity.clone());
        }
        let identity = content_hash(path)?.to_hex().to_string();
        if stamp(path)? != before {
            return None;
        }
        cache
            .lock()
            .ok()?
            .insert(path.to_path_buf(), (before, identity.clone()));
        Some(identity)
    }
    #[cfg(not(unix))]
    content_hash(path).map(|hash| hash.to_hex().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tool_contents_invalidate_identity_even_with_equal_size_and_mtime() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("tool");
        std::fs::write(&path, b"first").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let first = super::tool_identity(&path).unwrap();
        let fingerprint = super::content_fingerprint(vec![path.clone()]);
        #[cfg(unix)]
        let metadata = super::fingerprint(vec![path.clone()]);
        std::fs::write(&path, b"other").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_ne!(first, super::tool_identity(&path).unwrap());
        #[cfg(unix)]
        assert_ne!(metadata, super::fingerprint(vec![path.clone()]));
        assert_ne!(fingerprint, super::content_fingerprint(vec![path]));
    }
}
