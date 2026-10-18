//! Pinned inputs for the native build: downloaded once into the GPU cache, verified against the
//! SHA-256 upstream publishes, and extracted next to the archive.

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Pin {
    pub url: String,
    pub sha256: String,
}

impl Pin {
    pub fn from_table(table: &toml::Table, what: &str) -> Pin {
        let field = |name: &str| {
            table
                .get(name)
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("pins.toml: {what} has no `{name}`"))
                .to_string()
        };
        Pin {
            url: field("url"),
            sha256: field("sha256"),
        }
    }

    fn file_name(&self) -> &str {
        self.url.rsplit('/').next().unwrap_or(&self.url)
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// The verified archive for `pin` under `cache/downloads`, downloading it when absent.
pub fn download(cache: &Path, pin: &Pin, name: Option<&str>) -> PathBuf {
    let dest = cache
        .join("downloads")
        .join(name.unwrap_or_else(|| pin.file_name()));
    if dest.is_file() && sha256_file(&dest).ok().as_deref() == Some(pin.sha256.as_str()) {
        return dest;
    }
    fs::create_dir_all(dest.parent().unwrap()).expect("creating the GPU download cache");
    let partial = dest.with_extension("part");
    let fetched = (|| -> Result<(), String> {
        let response = ureq::get(&pin.url)
            .timeout(std::time::Duration::from_secs(900))
            .call()
            .map_err(|e| e.to_string())?;
        let mut reader = response.into_reader();
        let mut out = File::create(&partial).map_err(|e| e.to_string())?;
        io::copy(&mut reader, &mut out).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())
    })();
    if let Err(error) = fetched {
        let _ = fs::remove_file(&partial);
        panic!(
            "downloading {} failed: {error}\nplace the file (SHA-256 {}) at {} to build offline",
            pin.url,
            pin.sha256,
            dest.display()
        );
    }
    let actual = sha256_file(&partial).expect("hashing a download");
    if actual != pin.sha256 {
        let _ = fs::remove_file(&partial);
        panic!(
            "{}: SHA-256 mismatch (expected {}, got {actual})",
            pin.url, pin.sha256
        );
    }
    fs::rename(&partial, &dest).expect("finishing a download");
    dest
}

/// Written last into a finished cache directory. CI caches that prune `target/` can leave a
/// directory behind without its files; one without the marker is rebuilt.
const COMPLETE: &str = ".dream-complete";

fn reuse(dir: &Path) -> bool {
    if dir.join(COMPLETE).is_file() {
        return true;
    }
    if dir.exists() {
        fs::remove_dir_all(dir).expect("removing an incomplete cache directory");
    }
    false
}

fn finish(staging: &Path, dest: &Path) {
    File::create(staging.join(COMPLETE)).expect("marking a cache directory complete");
    fs::rename(staging, dest).expect("finishing a cache directory");
}

/// Extracts `archive` into `into` once, keeping only entries `keep` accepts. Returns `into`.
pub fn extract(archive: &Path, into: &Path, keep: impl Fn(&Path) -> bool) -> PathBuf {
    if reuse(into) {
        return into.to_path_buf();
    }
    let staging = into.with_extension("tmp");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).expect("creating an extraction directory");
    let name = archive.to_string_lossy();
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(File::open(archive).expect("opening a zip"))
            .unwrap_or_else(|e| panic!("{}: {e}", archive.display()));
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).expect("reading a zip entry");
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            if entry.is_dir() || !keep(&path) {
                continue;
            }
            let out = staging.join(&path);
            fs::create_dir_all(out.parent().unwrap()).expect("creating a directory");
            let mut file = File::create(&out).expect("creating an extracted file");
            io::copy(&mut entry, &mut file).expect("extracting a zip entry");
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&out, fs::Permissions::from_mode(mode)).ok();
            }
        }
    } else {
        let gz = flate2::read::GzDecoder::new(File::open(archive).expect("opening a tarball"));
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries().expect("reading a tarball") {
            let mut entry = entry.expect("reading a tar entry");
            let path = entry.path().expect("tar entry path").into_owned();
            if path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
                || !keep(&path)
            {
                continue;
            }
            entry
                .unpack_in(&staging)
                .unwrap_or_else(|e| panic!("{}: {e}", archive.display()));
        }
    }
    finish(&staging, into);
    into.to_path_buf()
}

/// The single top-level directory of an extracted archive, or the directory itself.
pub fn single_child(path: &Path) -> PathBuf {
    let children: Vec<_> = fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name() != COMPLETE)
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    match children.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => path.to_path_buf(),
    }
}

/// Copies `source` to `dest` and applies `patches` there, once per patch-set digest.
pub fn patched(source: &Path, dest: &Path, patches: &[PathBuf]) -> PathBuf {
    if reuse(dest) {
        return dest.to_path_buf();
    }
    let staging = dest.with_extension("tmp");
    let _ = fs::remove_dir_all(&staging);
    copy_tree(source, &staging);
    for patch in patches {
        // Without a ceiling git finds the Dream checkout above the cache and applies relative
        // to it; with one, `git apply` patches the plain extracted tree.
        let status = Command::new("git")
            .args(["apply", "-p1"])
            .arg(patch)
            .current_dir(&staging)
            .env("GIT_CEILING_DIRECTORIES", staging.parent().unwrap())
            .status()
            .expect("running `git apply` (git is required to patch slang-rhi)");
        assert!(status.success(), "applying {} failed", patch.display());
    }
    finish(&staging, dest);
    dest.to_path_buf()
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("creating a directory");
    for entry in fs::read_dir(from).expect("reading a directory") {
        let entry = entry.expect("reading a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copying a file");
        }
    }
}
