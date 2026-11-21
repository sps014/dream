//! Cache files for persisted reference-count insertion results, one per source module.

use crate::driver::{cache_budget, rt_stamp, toolchain::ToolchainConfig};
use dream_mir::ownership_cache::OwnershipCache;
use std::path::PathBuf;

pub(super) struct OwnershipStore {
    directory: PathBuf,
    partition: blake3::Hasher,
}

const MAX_MODULE_BYTES: usize = 64 * 1024 * 1024;

impl OwnershipStore {
    pub(super) fn new(
        config: &ToolchainConfig,
        project: &str,
        cfg_key: &str,
        options: impl std::fmt::Debug,
    ) -> Option<Self> {
        let compiler = rt_stamp::persistent_tool_identity(
            &std::env::current_exe().ok()?,
            &config.tool_identity_cache_root(),
        )?;
        let mut partition = blake3::Hasher::new();
        partition.update(b"dream-ownership-v1");
        partition.update(compiler.as_bytes());
        partition.update(project.as_bytes());
        partition.update(cfg_key.as_bytes());
        partition.update(format!("{options:?}").as_bytes());
        Some(Self {
            directory: config.prefix.join("cache").join("ownership"),
            partition,
        })
    }

    fn path(&self, module: &str) -> PathBuf {
        let mut key = self.partition.clone();
        key.update(module.as_bytes());
        self.directory.join(key.finalize().to_hex().as_str())
    }

    pub(super) fn load(&self, cache: &mut OwnershipCache, mir: &dream_mir::Mir) {
        for module in cache.modules(mir) {
            let path = self.path(&module);
            if let Some(payload) = super::checked_file::read(&path, MAX_MODULE_BYTES) {
                cache_budget::touch(&path);
                cache.load(&module, &payload);
            }
        }
    }

    pub(super) fn store(&self, cache: &OwnershipCache) {
        let changed = cache.changed();
        if changed.is_empty() || std::fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        for (module, payload) in changed {
            if payload.len() > MAX_MODULE_BYTES {
                continue;
            }
            super::checked_file::write(&self.path(module), &payload);
        }
    }
}
