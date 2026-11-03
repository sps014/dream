//! Size caps for directories Dream reuses across compiles. A cache hit refreshes an entry's
//! modification time; eviction removes the least recently used entries once a directory is over
//! its cap. Entries modified in the last few minutes stay, so a compile never deletes an object
//! another in-flight link still has open.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MIN_AGE: Duration = Duration::from_secs(15 * 60);
const CHECK_EVERY: Duration = Duration::from_secs(60);

pub(crate) const GENERATOR_RESULTS: u64 = 256 * 1024 * 1024;
pub(crate) const GENERATOR_EXES: u64 = 1024 * 1024 * 1024;
const GENERATOR_FAILED: u64 = 64 * 1024 * 1024;
const GENERATOR_OTHER: u64 = 256 * 1024 * 1024;
pub(crate) const RUNTIME_VERSIONS: u64 = 512 * 1024 * 1024;
pub(crate) const RUNTIME_UNITS: u64 = 512 * 1024 * 1024;
pub(crate) const STD_SOURCES: u64 = 32 * 1024 * 1024;
const TOOL_IDENTITIES: u64 = 64 * 1024 * 1024;

/// Caps every shared cache under this toolchain. Project-local generator executables are capped
/// from the generator build, which is the only place that knows their directory.
pub fn enforce_shared(config: &crate::driver::toolchain::ToolchainConfig) {
    let generators = config.generator_cache_root();
    if let Ok(children) = std::fs::read_dir(&generators) {
        for child in children.flatten() {
            let name = child.file_name();
            // Scratch files for a generator that is running right now.
            if name == "runs" {
                continue;
            }
            let budget = match name.to_str() {
                Some("results") => GENERATOR_RESULTS,
                Some("exe") => GENERATOR_EXES,
                Some("failed") => GENERATOR_FAILED,
                _ => GENERATOR_OTHER,
            };
            enforce(&child.path(), budget, &[]);
        }
    }
    let units = config.native_rt_cache_root().join("units");
    enforce(&units.join("versions"), RUNTIME_VERSIONS, &[]);
    enforce(&units, RUNTIME_UNITS, &["versions"]);
    enforce(&config.std_sources_root(), STD_SOURCES, &[]);
    enforce(&config.tool_identity_cache_root(), TOOL_IDENTITIES, &[]);
}

/// `dir/<name>` was just used. Refreshing its modification time keeps it ahead of unused entries.
pub fn touch(path: &Path) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let _ = file.set_modified(SystemTime::now());
}

pub fn enforce(dir: &Path, budget: u64, skip: &[&str]) {
    if !due(dir) {
        return;
    }
    let mut entries = entries(dir, skip);
    let mut total = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.bytes));
    if total <= budget {
        return;
    }
    let now = SystemTime::now();
    entries.sort_by(|a, b| {
        a.modified
            .cmp(&b.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    for entry in entries {
        if total <= budget {
            break;
        }
        let Some(age) = now.duration_since(entry.modified).ok() else {
            continue;
        };
        if age < MIN_AGE {
            continue;
        }
        let removed = if entry.directory {
            std::fs::remove_dir_all(&entry.path).is_ok()
        } else {
            std::fs::remove_file(&entry.path).is_ok()
        };
        if removed {
            if let Some(lock) = &entry.lock {
                let _ = std::fs::remove_file(lock);
            }
            total = total.saturating_sub(entry.bytes);
        }
    }
}

/// Measuring a directory walks every entry, so each compile process would otherwise pay for it.
/// The marker's modification time records the last check across processes.
fn due(dir: &Path) -> bool {
    let marker = dir.join(".budget-checked");
    let now = SystemTime::now();
    if std::fs::metadata(&marker)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|checked| now.duration_since(checked).ok())
        .is_some_and(|age| age < CHECK_EVERY)
    {
        return false;
    }
    if !dir.is_dir() {
        return false;
    }
    let _ = std::fs::write(&marker, b"");
    true
}

struct CacheEntry {
    path: PathBuf,
    lock: Option<PathBuf>,
    bytes: u64,
    modified: SystemTime,
    directory: bool,
}

fn entries(dir: &Path, skip: &[&str]) -> Vec<CacheEntry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut locks = std::collections::BTreeMap::new();
    let mut pending = Vec::new();
    for item in read.flatten() {
        let name = item.file_name();
        if skip.iter().any(|kept| name == *kept) {
            continue;
        }
        let text = name.to_string_lossy();
        if text.starts_with('.') || text.contains(".partial") || text.contains(".tmp") {
            continue;
        }
        if let Some(stem) = text.strip_suffix(".lock") {
            locks.insert(stem.to_string(), item.path());
            continue;
        }
        pending.push(item);
    }
    pending
        .into_iter()
        .filter_map(|item| {
            let meta = std::fs::symlink_metadata(item.path()).ok()?;
            let name = item.file_name().to_string_lossy().into_owned();
            let directory = meta.is_dir();
            Some(CacheEntry {
                lock: locks.remove(&name),
                bytes: if directory {
                    directory_bytes(&item.path())
                } else {
                    meta.len()
                },
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                directory,
                path: item.path(),
            })
        })
        .collect()
}

fn directory_bytes(dir: &Path) -> u64 {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    read.flatten()
        .map(|item| {
            let path = item.path();
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => directory_bytes(&path),
                Ok(meta) => meta.len(),
                Err(_) => 0,
            }
        })
        .fold(0, u64::saturating_add)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age(path: &Path, ago: Duration) {
        let file = std::fs::File::open(path).unwrap();
        file.set_modified(SystemTime::now() - ago).unwrap();
    }

    #[test]
    fn oldest_entries_go_first_and_recent_ones_stay() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("results");
        std::fs::create_dir(&dir).unwrap();
        for name in ["old", "older", "fresh"] {
            std::fs::write(dir.join(name), vec![1; 100]).unwrap();
        }
        age(&dir.join("older"), Duration::from_secs(3600));
        age(&dir.join("old"), Duration::from_secs(1800));
        enforce(&dir, 200, &[]);
        assert!(!dir.join("older").exists());
        assert!(dir.join("old").exists());
        assert!(dir.join("fresh").exists());
    }

    #[test]
    fn a_removed_directory_takes_its_lock_with_it() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("exe");
        let entry = dir.join("abc");
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(entry.join("gen.bin"), vec![1; 100]).unwrap();
        std::fs::write(dir.join("abc.lock"), b"lock").unwrap();
        age(&entry, Duration::from_secs(3600));
        enforce(&dir, 10, &[]);
        assert!(!entry.exists());
        assert!(!dir.join("abc.lock").exists());
    }

    #[test]
    fn in_progress_and_named_children_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        std::fs::write(dir.join("unit.partial"), vec![1; 100]).unwrap();
        std::fs::create_dir(dir.join(".staging-1")).unwrap();
        std::fs::write(dir.join(".staging-1").join("x"), vec![1; 100]).unwrap();
        std::fs::create_dir(dir.join("versions")).unwrap();
        std::fs::write(dir.join("versions").join("kept"), vec![1; 100]).unwrap();
        age(&dir.join("unit.partial"), Duration::from_secs(3600));
        age(&dir.join(".staging-1"), Duration::from_secs(3600));
        age(&dir.join("versions"), Duration::from_secs(3600));
        enforce(dir, 0, &["versions"]);
        assert!(dir.join("unit.partial").exists());
        assert!(dir.join(".staging-1").exists());
        assert!(dir.join("versions").join("kept").exists());
    }
}
