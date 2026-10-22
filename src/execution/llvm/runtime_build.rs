//! The cache skeleton the native and wasm32 runtime builds share: a directory lock, a stamp over
//! the inputs plus the headers the previous build discovered, an output integrity stamp, the
//! signature extraction step, and a snapshot that outlives the lock.

use super::runtime::{disassemble, reduce_disassembly};
use super::tools::LlvmTools;
use crate::driver::rt_stamp;
use crate::driver::wasi::run_captured;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// One C (or assembly) runtime source with its own defines and include directories.
pub(super) struct Unit {
    pub path: PathBuf,
    pub defines: Vec<String>,
    pub include_dirs: Vec<PathBuf>,
}

/// A locked runtime cache directory. The lock is held until the value is dropped.
pub(super) struct RuntimeCache {
    dir: PathBuf,
    _lock: File,
    previous: Option<Vec<PathBuf>>,
}

impl RuntimeCache {
    pub(super) fn open(dir: &Path) -> Result<Self, String> {
        let io = |e: std::io::Error| format!("{}: {e}", dir.display());
        std::fs::create_dir_all(dir).map_err(io)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(".lock"))
            .map_err(io)?;
        lock.lock().map_err(io)?;
        let previous = std::fs::read(dir.join(".dependencies.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok());
        Ok(Self {
            dir: dir.to_path_buf(),
            _lock: lock,
            previous,
        })
    }

    /// Headers and sources the previous build read, to fold into this build's fingerprint.
    pub(super) fn previous_dependencies(&self) -> &[PathBuf] {
        self.previous.as_deref().unwrap_or_default()
    }

    /// True when every output exists unchanged and was built from `fingerprint`.
    pub(super) fn is_fresh(&self, outputs: &[PathBuf], fingerprint: &str) -> bool {
        self.previous.is_some()
            && outputs.iter().all(|o| o.exists())
            && rt_stamp::matches(
                &self.dir.join(".outputs"),
                &rt_stamp::content_fingerprint(outputs.to_vec()),
            )
            && rt_stamp::matches(&self.dir.join(".stamp"), fingerprint)
    }

    /// Records a finished build so the next call with the same fingerprint reuses it.
    pub(super) fn commit(
        &self,
        dependencies: &[PathBuf],
        outputs: &[PathBuf],
        fingerprint: &str,
    ) -> Result<(), String> {
        let io = |e: std::io::Error| format!("{}: {e}", self.dir.display());
        std::fs::write(
            self.dir.join(".dependencies.json"),
            serde_json::to_vec(dependencies).map_err(|e| e.to_string())?,
        )
        .map_err(io)?;
        std::fs::write(
            self.dir.join(".outputs"),
            rt_stamp::content_fingerprint(outputs.to_vec()),
        )
        .map_err(io)?;
        std::fs::write(self.dir.join(".stamp"), fingerprint).map_err(io)
    }

    /// Immutable copies of `outputs` that later builds of this directory cannot replace.
    pub(super) fn snapshot(&self, outputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
        super::runtime_snapshot::publish(&self.dir, outputs)
    }
}

/// Writes the reduced declarations of `bc` plus the header anchor unit to `sigs`.
pub(super) fn write_signatures(
    tools: &LlvmTools,
    bc: &Path,
    anchor: &Path,
    sigs: &Path,
    label: &str,
    transform: impl Fn(&str) -> String,
) -> Result<(), String> {
    let merged = bc.with_file_name("sigs.bc");
    let mut link = tools.command("llvm-link");
    link.arg(bc).arg(anchor).arg("-o").arg(&merged);
    run_captured(&mut link, label)?;
    let text = transform(&disassemble(tools, &merged)?);
    let _ = std::fs::remove_file(&merged);
    let _ = std::fs::remove_file(anchor);
    std::fs::write(sigs, reduce_disassembly(&text)).map_err(|e| format!("{}: {e}", sigs.display()))
}
