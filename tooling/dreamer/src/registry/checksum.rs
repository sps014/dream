use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

/// Computes the `sha256:<hex>` checksum string used throughout the registry protocol and
/// lockfile (`IndexEntry::cksum`, `LockedPackage::checksum`).
pub fn sha256_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// Verifies `bytes` matches `expected` (a `sha256:<hex>` string), erroring with both checksums on
/// mismatch so a corrupted/tampered download is never silently accepted.
pub fn verify(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = sha256_of(bytes);
    if actual != expected {
        bail!("checksum mismatch: expected {}, got {}", expected, actual);
    }
    Ok(())
}

pub fn verify_file(path: &std::path::Path, expected: &str) -> Result<()> {
    use std::io::Read;
    let mut reader = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut total = 0usize;
    let mut buffer = [0; 16384];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n)
            .ok_or_else(|| anyhow::anyhow!("tarball size overflow"))?;
        if total > super::MAX_TARBALL_BYTES {
            bail!("tarball exceeds registry download limit");
        }
        hash.update(&buffer[..n]);
    }
    if format!("sha256:{:x}", hash.finalize()) != expected {
        bail!("cached archive checksum mismatch");
    }
    Ok(())
}

/// Download to an isolated file; only a bounded, verified body becomes a cache entry.
pub fn copy_verified(
    mut reader: impl std::io::Read,
    dest: &std::path::Path,
    expected: &str,
) -> Result<()> {
    use std::io::Write;
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cache destination has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = [0u8; 16384];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n)
            .ok_or_else(|| anyhow::anyhow!("tarball size overflow"))?;
        if total > super::MAX_TARBALL_BYTES {
            bail!("tarball exceeds registry download limit");
        }
        hasher.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
    }
    let actual = format!("sha256:{:x}", hasher.finalize());
    if actual != expected {
        bail!("checksum mismatch: expected {}, got {}", expected, actual);
    }
    output.as_file().sync_all()?;
    output.persist(dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_streams_never_publish_partial_or_bad_downloads() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("package.tar.gz");
        std::fs::write(&dest, b"existing").unwrap();
        let oversized = std::io::repeat(0).take(super::super::MAX_TARBALL_BYTES as u64 + 1);
        assert!(copy_verified(oversized, &dest, "sha256:invalid").is_err());
        assert!(copy_verified(&b"bad"[..], &dest, &sha256_of(b"good")).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"existing");
        copy_verified(&b"good"[..], &dest, &sha256_of(b"good")).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"good");
    }
    use std::io::Read;
}
