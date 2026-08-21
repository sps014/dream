//! Pre-compressed artifact emission for release builds: writes `.gz` / `.br` siblings next
//! to the shipped `.wasm` and `*.runtime.js` so static servers with `gzip_static` /
//! `brotli_static` (nginx, CDNs) can serve them without on-the-fly compression. Browsers
//! never compress on their own — without server (or sidecar) support the raw bytes ship.

use std::fs;
use std::io::Write;
use std::path::Path;
use tracing::info;

/// Files smaller than this are not worth a sidecar.
const MIN_COMPRESS_BYTES: u64 = 1024;

/// Writes `<path>.gz` and `<path>.br` next to `path`. Returns `(gzip_len, brotli_len)`.
/// Best-effort per encoding: one failing writer does not block the other.
pub fn write_precompressed(path: &Path) -> std::io::Result<(u64, u64)> {
    let raw = fs::read(path)?;
    let raw_len = raw.len() as u64;
    if raw_len < MIN_COMPRESS_BYTES {
        return Ok((0, 0));
    }
    let mut gz_len = 0u64;
    let mut br_len = 0u64;
    match write_gzip(path, &raw) {
        Ok(n) => gz_len = n,
        Err(e) => tracing::warn!("could not write {}: {}", gzip_path(path).display(), e),
    }
    match write_brotli(path, &raw) {
        Ok(n) => br_len = n,
        Err(e) => tracing::warn!("could not write {}: {}", brotli_path(path).display(), e),
    }
    info!(
        "precompressed {} (raw {}, gzip {}, brotli {})",
        path.display(),
        raw.len(),
        gz_len,
        br_len
    );
    Ok((gz_len, br_len))
}

fn gzip_path(path: &Path) -> std::path::PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".gz");
    std::path::PathBuf::from(p)
}

fn brotli_path(path: &Path) -> std::path::PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".br");
    std::path::PathBuf::from(p)
}

fn write_gzip(path: &Path, raw: &[u8]) -> std::io::Result<u64> {
    let out_path = gzip_path(path);
    let mut encoder = flate2::write::GzEncoder::new(
        fs::File::create(&out_path)?,
        flate2::Compression::best(),
    );
    encoder.write_all(raw)?;
    encoder.finish()?;
    Ok(fs::metadata(&out_path)?.len())
}

fn write_brotli(path: &Path, raw: &[u8]) -> std::io::Result<u64> {
    let out_path = brotli_path(path);
    let mut out = fs::File::create(&out_path)?;
    // Quality 11 / lgwin 22 = maximum compression; build-time only, so speed is fine.
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 22,
        ..Default::default()
    };
    let mut compressor = brotli::CompressorWriter::with_params(&mut out, 4096, &params);
    compressor.write_all(raw)?;
    drop(compressor);
    out.flush()?;
    Ok(fs::metadata(&out_path)?.len())
}
