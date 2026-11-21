//! Cache files that carry a blake3 checksum of their payload and are replaced atomically, so a
//! torn or concurrent write is never read back.

use std::path::Path;

/// The payload of `path`, or `None` when it is missing, larger than `max` or corrupt.
pub(super) fn read(path: &Path, max: usize) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take((max + 33) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let (checksum, payload) = bytes.split_at_checked(32)?;
    (payload.len() <= max && blake3::hash(payload).as_bytes() == checksum)
        .then(|| payload.to_vec())
}

pub(super) fn write(path: &Path, payload: &[u8]) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let checksum = blake3::hash(payload);
    let record = [checksum.as_bytes().as_slice(), payload].concat();
    let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let partial = path.with_extension(format!("{}.{nonce}.partial", std::process::id()));
    if std::fs::write(&partial, record).is_err() || std::fs::rename(&partial, path).is_err() {
        let _ = std::fs::remove_file(partial);
    }
}
