//! Pinned native build archives: transient failures retry, verified contents alone enter the cache.

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

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

enum DownloadError {
    Request(Box<ureq::Error>),
    Io(io::Error),
}

impl DownloadError {
    fn retryable(&self) -> bool {
        match self {
            Self::Request(error) => match error.as_ref() {
                ureq::Error::Status(code, _) => matches!(code, 408 | 429 | 500 | 502 | 503 | 504),
                ureq::Error::Transport(_) => true,
            },
            Self::Io(error) => matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::Interrupted
                    | io::ErrorKind::WouldBlock
            ),
        }
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
        }
    }
}

fn fetch(agent: &ureq::Agent, url: &str, partial: &Path) -> Result<(), DownloadError> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| DownloadError::Request(Box::new(e)))?;
    let mut reader = response.into_reader();
    let mut out = File::create(partial).map_err(DownloadError::Io)?;
    io::copy(&mut reader, &mut out).map_err(DownloadError::Io)?;
    out.flush().map_err(DownloadError::Io)
}

/// The verified archive for `pin` under `cache/downloads`, downloading it when absent.
pub fn download(cache: &Path, pin: &Pin, name: Option<&str>) -> PathBuf {
    let dest = cache
        .join("downloads")
        .join(name.unwrap_or_else(|| pin.file_name()));
    if dest.is_file() {
        if sha256_file(&dest).ok().as_deref() == Some(pin.sha256.as_str()) {
            return dest;
        }
        // Windows cannot rename the verified replacement over an existing corrupt archive.
        fs::remove_file(&dest).expect("removing an invalid cached GPU archive");
    }
    fs::create_dir_all(dest.parent().unwrap()).expect("creating the GPU download cache");
    let partial = dest.with_extension("part");
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(30))
        .timeout(Duration::from_secs(900))
        .build();
    for attempt in 1..=4 {
        match fetch(&agent, &pin.url, &partial) {
            Ok(()) => break,
            Err(error) => {
                let _ = fs::remove_file(&partial);
                if attempt == 4 || !error.retryable() {
                    panic!(
                        "downloading {} failed after {attempt} attempt(s): {error}\nplace the file (SHA-256 {}) at {} to build offline",
                        pin.url,
                        pin.sha256,
                        dest.display()
                    );
                }
                let delay = attempt * 2;
                println!(
                    "cargo:warning=GET {} failed ({error}); retrying in {delay}s ({attempt}/4)",
                    pin.url
                );
                std::thread::sleep(Duration::from_secs(delay));
            }
        }
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
