use super::{
    dream_prefix, is_installed, tool_binary, tool_dir, toolchains_dir, toolchains_env_path,
};
use crate::fetch::cache_dir;
use anyhow::{Context, Result, bail};
use dream_abi::toolchain::{ArchiveKind, HostArchive, PINNED_TOOLS, PinnedTool};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn install(tools: &[&'static PinnedTool]) -> Result<()> {
    let host = super::host_key()?;
    fs::create_dir_all(toolchains_dir())?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(toolchains_dir().join(".install.lock"))?;
    lock.lock()?;
    for tool in tools {
        if is_installed(tool) {
            if tool.name == "slang" && !super::slang_layout::is_current(&tool_dir(tool)) {
                println!("Rebuilding dream-slang-layout for the current recorder sources …");
                super::slang_layout::build(&tool_dir(tool))?;
            }
            if tool.name == "slang" {
                stage_directx_compilers(&tool_dir(tool))?;
            }
            println!(
                "{} already installed at {}",
                tool.name,
                tool_dir(tool).display()
            );
            continue;
        }
        let Some(archive) = tool.archive(host) else {
            bail!(
                "{} has no download for {host}: {}",
                tool.name,
                tool.install_hint
            );
        };
        install_one(tool, archive)?;
    }
    write_toolchains_env()?;
    Ok(())
}

pub fn list() -> Result<()> {
    println!("prefix: {}", dream_prefix().display());
    println!("toolchains: {}", toolchains_dir().display());
    for tool in PINNED_TOOLS {
        let status = if is_installed(tool) {
            format!("installed ({})", tool_dir(tool).display())
        } else if tool.archives.is_empty() {
            format!("not installed ({})", tool.install_hint)
        } else {
            "not installed".to_string()
        };
        println!("  {:<8} {:<10} {}", tool.name, tool.version, status);
    }
    Ok(())
}

pub fn uninstall(tool: &PinnedTool) -> Result<()> {
    let dir = tool_dir(tool);
    if dir.is_dir() {
        fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        println!("removed {}", dir.display());
    } else {
        println!("{} was not installed", tool.name);
    }
    write_toolchains_env()?;
    Ok(())
}

fn archive_filename(archive: &HostArchive) -> &str {
    archive.url.rsplit('/').next().unwrap_or(archive.url)
}

fn install_one(tool: &PinnedTool, archive: &HostArchive) -> Result<()> {
    let dest = tool_dir(tool);
    println!("Downloading {} …", archive.url);
    let cache = download_verified(archive)?;
    println!("Extracting to {} …", dest.display());
    extract_archive(&cache, archive.kind, &dest)?;
    if tool.name == "slang" {
        println!("Building dream-slang-layout …");
        super::slang_layout::build(&dest)?;
        stage_directx_compilers(&dest)?;
    }
    if !is_installed(tool) {
        bail!(
            "extracted {} but did not find the expected binary under {}",
            archive_filename(archive),
            dest.display()
        );
    }
    println!(
        "Installed {} {} ({})",
        tool.name,
        tool.version,
        dest.display()
    );
    Ok(())
}

fn download_verified(archive: &HostArchive) -> Result<PathBuf> {
    let cache = cache_dir().join("toolchains");
    fs::create_dir_all(&cache)?;
    let filename = archive_filename(archive);
    let path = cache.join(filename);
    let expected = format!("sha256:{}", archive.sha256);
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
    let tmp = path.with_extension("part");
    let mut failure = None;
    for attempt in 1..=4 {
        match fetch_archive(&agent, archive, &tmp) {
            Ok(actual) if actual == expected => {
                fs::rename(&tmp, &path)?;
                return Ok(path);
            }
            Ok(actual) => {
                let _ = fs::remove_file(&tmp);
                bail!("checksum mismatch for {filename}: expected {expected}, got {actual}");
            }
            Err(error) if attempt < 4 && retryable_download(&error) => {
                let _ = fs::remove_file(&tmp);
                eprintln!("GET {} failed ({error}); retrying", archive.url);
                std::thread::sleep(Duration::from_secs(attempt * 2));
                failure = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(failure.unwrap_or_else(|| anyhow::anyhow!("GET {} failed", archive.url)))
}

fn fetch_archive(agent: &ureq::Agent, archive: &HostArchive, tmp: &Path) -> Result<String> {
    let resp = agent
        .get(archive.url)
        .call()
        .with_context(|| format!("GET {}", archive.url))?;
    let mut reader = resp.into_reader();
    let mut file = File::create(tmp)?;
    let actual = stream_copy_sha256(&mut reader, &mut file)?;
    file.flush()?;
    Ok(actual)
}

fn retryable_download(error: &anyhow::Error) -> bool {
    match error.downcast_ref::<ureq::Error>() {
        Some(ureq::Error::Status(code, _)) => matches!(*code, 408 | 429 | 500 | 502 | 503 | 504),
        Some(ureq::Error::Transport(_)) => true,
        None => false,
    }
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
        ArchiveKind::TarXz => unpack_tar_xz(archive, &tmp)?,
        ArchiveKind::TarGz => {
            tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?)).unpack(&tmp)?
        }
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

/// The path `DREAM_<TOOL>` takes for an installed tool, matching its override kind.
fn override_value(tool: &PinnedTool) -> PathBuf {
    match tool.override_kind {
        dream_abi::toolchain::OverrideKind::Executable => tool_binary(tool),
        dream_abi::toolchain::OverrideKind::Root => tool_dir(tool),
    }
}

fn write_toolchains_env() -> Result<()> {
    let path = toolchains_env_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = String::from("# Written by `dreamer toolchain install`. Sourced from env.sh.\n");
    for tool in PINNED_TOOLS {
        if is_installed(tool) {
            body.push_str(&format!(
                "{}={}\n",
                tool.env,
                override_value(tool).display()
            ));
        }
    }
    fs::write(&path, body)?;
    println!("Wrote {}", path.display());
    Ok(())
}

/// DXIL targets need the Windows SDK's `dxcompiler.dll` and `dxil.dll` inside the Slang `bin`
/// directory, because the generator sandbox can read only that root. Other hosts have nothing
/// to copy.
fn stage_directx_compilers(slang_root: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let dest = slang_root.join("bin");
        fs::create_dir_all(&dest)?;
        let Some(sdk_bin) = windows_dxil_bin() else {
            println!(
                "Windows SDK shader compiler not found. DXIL targets need dxcompiler.dll and \
                 dxil.dll in {}. Install the Windows SDK, or run this from a Visual Studio \
                 Developer shell so WindowsSdkVerBinPath is set, then rerun `dreamer toolchain \
                 install slang`.",
                dest.display()
            );
            return Ok(());
        };
        for library in ["dxcompiler.dll", "dxil.dll"] {
            let source = sdk_bin.join(library);
            if !source.is_file() {
                bail!(
                    "Windows SDK directory {} has no {library}",
                    sdk_bin.display()
                );
            }
            fs::copy(&source, dest.join(library))
                .with_context(|| format!("copying {library} into {}", dest.display()))?;
            println!("Copied {library} into {}", dest.display());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = slang_root;
    }
    Ok(())
}

/// `bin/<version>/<arch>` of the newest Windows 10 SDK that contains `dxcompiler.dll`.
/// `WindowsSdkVerBinPath` (set by a Visual Studio Developer shell) wins over the registry.
#[cfg(windows)]
fn windows_dxil_bin() -> Option<PathBuf> {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    if let Some(bin) = std::env::var_os("WindowsSdkVerBinPath") {
        let dir = PathBuf::from(bin).join(arch);
        if dir.join("dxcompiler.dll").is_file() {
            return Some(dir);
        }
    }
    kits_root_from_registry().and_then(|root| newest_dxil_bin(&root, arch))
}

#[cfg(windows)]
fn kits_root_from_registry() -> Option<PathBuf> {
    for key in [
        r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots",
    ] {
        let output = std::process::Command::new("reg")
            .args(["query", key, "/v", "KitsRoot10"])
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(path) = text.split("REG_SZ").nth(1) {
            let root = PathBuf::from(path.trim());
            if root.is_dir() {
                return Some(root);
            }
        }
    }
    None
}

/// The highest `bin/<version>/<arch>` under a Windows Kits root that contains `dxcompiler.dll`.
#[cfg(any(windows, test))]
fn newest_dxil_bin(kits_root: &Path, arch: &str) -> Option<PathBuf> {
    let mut versions: Vec<_> = fs::read_dir(kits_root.join("bin"))
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    versions.sort_by(|left, right| cmp_sdk_version(left, right));
    versions.into_iter().rev().find_map(|version| {
        let dir = version.join(arch);
        dir.join("dxcompiler.dll").is_file().then_some(dir)
    })
}

#[cfg(any(windows, test))]
fn cmp_sdk_version(left: &Path, right: &Path) -> std::cmp::Ordering {
    let parts = |path: &Path| -> Vec<u32> {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    parts(left).cmp(&parts(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tar_xz_flattens_single_root() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tar_bytes = Vec::new();
        {
            let mut b = tar::Builder::new(&mut tar_bytes);
            let mut hdr = tar::Header::new_gnu();
            hdr.set_size(3);
            hdr.set_cksum();
            b.append_data(&mut hdr, "tool-1/bin/hello.txt", b"hi\n" as &[u8])
                .unwrap();
            b.finish().unwrap();
        }
        let archive = tmp.path().join("tool.tar.xz");
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut tar_bytes.as_slice(), &mut xz).unwrap();
        fs::write(&archive, xz).unwrap();
        let dest = tmp.path().join("out");
        extract_archive(&archive, ArchiveKind::TarXz, &dest).unwrap();
        assert!(dest.join("bin/hello.txt").is_file());
        assert!(!dest.join("tool-1").exists());
    }

    #[test]
    fn newest_windows_sdk_bin_wins_when_it_has_the_shader_compiler() {
        let tmp = tempfile::tempdir().unwrap();
        let older = tmp.path().join("bin/10.0.19041.0/x64");
        let newer = tmp.path().join("bin/10.0.22621.0/x64");
        fs::create_dir_all(&older).unwrap();
        fs::create_dir_all(&newer).unwrap();
        fs::write(older.join("dxcompiler.dll"), "old").unwrap();
        fs::write(newer.join("dxcompiler.dll"), "new").unwrap();
        fs::create_dir_all(tmp.path().join("bin/10.0.26100.0/arm64")).unwrap();
        assert_eq!(newest_dxil_bin(tmp.path(), "x64").unwrap(), newer);
        assert!(newest_dxil_bin(tmp.path(), "arm64").is_none());
    }
}
