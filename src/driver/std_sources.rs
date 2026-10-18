//! The embedded stdlib written to disk for debuggers. Debug info points stdlib frames here, so
//! lldb can show their source and bind breakpoints set in these files.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn embedded() -> impl Iterator<Item = (&'static str, &'static str)> {
    dream_stdlib::STD_PACKAGES.iter().flat_map(|pkg| {
        pkg.files.iter().filter_map(|&(vpath, source)| {
            vpath
                .strip_prefix(dream_stdlib::STD_PATH_PREFIX)
                .map(|rel| (rel, source))
        })
    })
}

/// Content digest of every embedded stdlib file, so each compiler build gets its own directory
/// and a directory's contents never change once written.
fn digest() -> &'static str {
    static DIGEST: OnceLock<String> = OnceLock::new();
    DIGEST.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        for (rel, source) in embedded() {
            for part in [rel, source] {
                hash.update(&(part.len() as u64).to_le_bytes());
                hash.update(part.as_bytes());
            }
        }
        hash.finalize().to_hex()[..16].to_string()
    })
}

/// Returns `<root>/<digest>`, writing the stdlib there first when it is not already complete.
/// The tree is staged in a sibling directory and renamed into place, so concurrent compiles
/// never observe a partial one.
pub fn materialize(root: &Path) -> io::Result<PathBuf> {
    let dir = root.join(digest());
    if dir.is_dir() {
        return Ok(dir);
    }
    // Generator executables compile in parallel inside the same compiler process.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let staged = root.join(format!(
        ".staging-{}-{}-{}",
        digest(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&staged);
    for (rel, source) in embedded() {
        let target = staged.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, source)?;
    }
    match std::fs::rename(&staged, &dir) {
        Ok(()) => Ok(dir),
        Err(_) if dir.is_dir() => {
            let _ = std::fs::remove_dir_all(&staged);
            Ok(dir)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staged);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_compiles_publish_complete_source_trees() {
        let root = tempfile::tempdir().unwrap();
        let barrier = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    barrier.wait();
                    let dir = materialize(root.path()).unwrap();
                    for (relative, source) in embedded() {
                        assert_eq!(std::fs::read_to_string(dir.join(relative)).unwrap(), source);
                    }
                });
            }
        });
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn writes_every_std_file_once() {
        let root = tempfile::tempdir().unwrap();
        let dir = materialize(root.path()).unwrap();
        assert_eq!(materialize(root.path()).unwrap(), dir);
        let list = dir.join("system/collections/list.dream");
        let embedded_list = embedded()
            .find(|(rel, _)| *rel == "system/collections/list.dream")
            .unwrap()
            .1;
        assert_eq!(std::fs::read_to_string(list).unwrap(), embedded_list);
        let entries = std::fs::read_dir(root.path()).unwrap().count();
        assert_eq!(entries, 1, "staging directories are cleaned up");
    }
}
