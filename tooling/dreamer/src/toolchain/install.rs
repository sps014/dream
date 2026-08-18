use super::catalog::{self, ArchiveKind, Artifact};
use super::{
    dream_prefix, is_installed, toolchains_dir, toolchains_env_path, wasi_sdk_dir, zig_binary,
    zig_dir, Component, Host,
};
use crate::fetch::cache_dir;
use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn install(components: &[Component]) -> Result<()> {
    let host = super::detect_host()?;
    for component in components {
        catalog::ensure_host_supported(host)?;
        if is_installed(*component, host) {
            println!(
                "{} already installed at {}",
                component.id(),
                match component {
                    Component::Cc => zig_dir(),
                    Component::WasiSdk => wasi_sdk_dir(host),
                }
                .display()
            );
            continue;
        }
        install_one(*component, host)?;
    }
    write_toolchains_env(host)?;
    Ok(())
}

pub fn list() -> Result<()> {
    let host = super::detect_host()?;
    println!("prefix: {}", dream_prefix().display());
    println!("toolchains: {}", toolchains_dir().display());
    for c in Component::all() {
        let status = if is_installed(c, host) {
            match c {
                Component::Cc => format!("installed ({})", zig_dir().display()),
                Component::WasiSdk => format!("installed ({})", wasi_sdk_dir(host).display()),
            }
        } else {
            "not installed".to_string()
        };
        println!("  {:<8} {}", c.id(), status);
    }
    Ok(())
}

pub fn uninstall(component: Component) -> Result<()> {
    let host = super::detect_host()?;
    let dir = catalog::dest_dir(component, host)?;
    if dir.is_dir() {
        fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        println!("removed {}", dir.display());
    } else {
        println!("{} was not installed", component.id());
    }
    write_toolchains_env(host)?;
    Ok(())
}

fn install_one(component: Component, host: Host) -> Result<()> {
    let artifact = catalog::artifact_for(component, host)?;
    let dest = catalog::dest_dir(component, host)?;
    println!("Downloading {} …", artifact.url);
    let cache = download_verified(&artifact)?;
    println!("Extracting to {} …", dest.display());
    extract_archive(&cache, artifact.kind, &dest)?;
    if !is_installed(component, host) {
        bail!(
            "extracted {} but did not find the expected binary under {}",
            artifact.filename,
            dest.display()
        );
    }
    println!("Installed {} ({})", component.id(), dest.display());
    Ok(())
}

fn download_verified(artifact: &Artifact) -> Result<PathBuf> {
    let cache = cache_dir().join("toolchains");
    fs::create_dir_all(&cache)?;
    let path = cache.join(&artifact.filename);
    let expected = format!("sha256:{}", artifact.sha256);
    if path.is_file() {
        let mut f = File::open(&path)?;
        if stream_sha256(&mut f)? == expected {
            return Ok(path);
        }
        let _ = fs::remove_file(&path);
    }
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build();
    let resp = agent
        .get(&artifact.url)
        .call()
        .with_context(|| format!("GET {}", artifact.url))?;
    let mut reader = resp.into_reader();
    let tmp = path.with_extension("part");
    let mut file = File::create(&tmp)?;
    let actual = stream_copy_sha256(&mut reader, &mut file)?;
    file.flush()?;
    drop(file);
    if actual != expected {
        let _ = fs::remove_file(&tmp);
        bail!(
            "checksum mismatch for {}: expected {expected}, got {actual}",
            artifact.filename
        );
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}

fn stream_sha256(reader: &mut impl io::Read) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn stream_copy_sha256(reader: &mut impl io::Read, writer: &mut impl Write) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn extract_archive(archive: &Path, kind: ArchiveKind, dest: &Path) -> Result<()> {
    let tmp_name = format!(
        "{}.tmp-extract",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("tc")
    );
    let tmp = dest.with_file_name(tmp_name);
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    match kind {
        ArchiveKind::TarGz => unpack_tar_gz(archive, &tmp)?,
        ArchiveKind::TarXz => unpack_tar_xz(archive, &tmp)?,
        ArchiveKind::Zip => unpack_zip(archive, &tmp)?,
    }
    let unpacked = flatten_single_dir(&tmp)?;
    if dest.is_dir() {
        fs::remove_dir_all(dest)?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(&unpacked, dest).is_err() {
        copy_dir_all(&unpacked, dest)?;
        fs::remove_dir_all(&unpacked)?;
    }
    let _ = fs::remove_dir_all(&tmp);
    Ok(())
}

fn flatten_single_dir(tmp: &Path) -> Result<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(tmp)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    if entries.len() == 1 && entries[0].is_dir() {
        return Ok(entries[0].clone());
    }
    Ok(tmp.to_path_buf())
}

fn unpack_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(dest)?;
    Ok(())
}

fn unpack_tar_xz(archive: &Path, dest: &Path) -> Result<()> {
    let compressed = File::open(archive)?;
    let mut compressed = io::BufReader::new(compressed);
    let tar_path = dest.join(".inner.tar");
    let mut tar_file = File::create(&tar_path)?;
    lzma_rs::xz_decompress(&mut compressed, &mut tar_file)
        .map_err(|e| anyhow::anyhow!("xz decompress: {e}"))?;
    drop(tar_file);
    let tar_file = File::open(&tar_path)?;
    let mut tar = tar::Archive::new(tar_file);
    tar.unpack(dest)?;
    let _ = fs::remove_file(&tar_path);
    Ok(())
}

fn unpack_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = enclosed_zip_path(&entry) else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut outfile = File::create(&out)?;
        io::copy(&mut entry, &mut outfile)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                fs::set_permissions(&out, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}

fn enclosed_zip_path(entry: &zip::read::ZipFile<'_>) -> Option<PathBuf> {
    let name = entry.enclosed_name()?;
    if name
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(name.to_path_buf())
}

fn copy_dir_all(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn write_toolchains_env(host: Host) -> Result<()> {
    let path = toolchains_env_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = String::from("# Written by `dreamer toolchain install`. Sourced from env.sh.\n");
    if zig_binary().is_file() {
        body.push_str(&format!("DREAM_ZIG={}\n", zig_binary().display()));
    }
    if super::wasi_clang(host).is_file() {
        body.push_str(&format!("WASI_SDK_PATH={}\n", wasi_sdk_dir(host).display()));
    }
    fs::write(&path, body)?;
    println!("Wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    #[test]
    fn extract_tar_gz_flattens_single_root() {
        let tmp = tempfile::tempdir().unwrap();
        let tar_path = tmp.path().join("sdk.tar.gz");
        {
            let file = File::create(&tar_path).unwrap();
            let enc = GzEncoder::new(file, Compression::default());
            let mut b = tar::Builder::new(enc);
            let mut hdr = tar::Header::new_gnu();
            hdr.set_size(3);
            hdr.set_cksum();
            b.append_data(&mut hdr, "sdk-1/bin/hello.txt", b"hi\n" as &[u8])
                .unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let dest = tmp.path().join("out");
        extract_archive(&tar_path, ArchiveKind::TarGz, &dest).unwrap();
        assert!(dest.join("bin/hello.txt").is_file());
        assert!(!dest.join("sdk-1").exists());
    }
}
