use crate::driver::rt_stamp;
use std::path::{Path, PathBuf};

/// Consumers outlive the build lock, so their inputs must never be replaced by another build.
pub(super) fn publish(root: &Path, files: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut hasher = blake3::Hasher::new();
    let mut hashes = Vec::with_capacity(files.len());
    for file in files {
        let name = file.file_name().ok_or("runtime output has no filename")?;
        let bytes = name.as_encoded_bytes();
        let hash = rt_stamp::content_hash(file).ok_or("unreadable runtime output")?;
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
        hasher.update(hash.as_bytes());
        hashes.push(hash);
    }
    let directory = root
        .join("versions")
        .join(hasher.finalize().to_hex().as_str());
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut outputs = Vec::with_capacity(files.len());
    for (file, hash) in files.iter().zip(hashes) {
        let output = directory.join(file.file_name().ok_or("runtime output has no filename")?);
        if rt_stamp::content_hash(&output) != Some(hash) {
            let partial = output.with_extension("partial");
            std::fs::copy(file, &partial).map_err(|e| e.to_string())?;
            std::fs::OpenOptions::new()
                .write(true)
                .open(&partial)
                .and_then(|file| file.sync_all())
                .map_err(|e| e.to_string())?;
            std::fs::rename(partial, &output).map_err(|e| e.to_string())?;
        }
        outputs.push(output);
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    #[test]
    fn later_builds_cannot_replace_an_acquired_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("runtime.bc");
        std::fs::write(&file, b"first").unwrap();
        let first = super::publish(dir.path(), std::slice::from_ref(&file)).unwrap();
        std::fs::write(&file, b"second").unwrap();
        let second = super::publish(dir.path(), std::slice::from_ref(&file)).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first[0]).unwrap(), b"first");
        assert_eq!(std::fs::read(&second[0]).unwrap(), b"second");
        std::fs::write(&second[0], b"corrupt").unwrap();
        assert_eq!(second, super::publish(dir.path(), &[file]).unwrap());
        assert_eq!(std::fs::read(&second[0]).unwrap(), b"second");
    }
}
