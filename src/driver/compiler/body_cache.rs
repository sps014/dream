//! Persists successful body checks and reusable typed bodies by source module.

use super::load::LoadedProgram;
use crate::driver::{cache_budget, rt_stamp, toolchain::ToolchainConfig};
use dream_sema::{analyzer::TypedBodies, module_graph::ModuleHashes};
use dream_types::ModuleId;
use indexmap::{IndexMap, IndexSet};
use std::path::PathBuf;

pub(super) struct BodyCache {
    directory: PathBuf,
    /// Each module's cache file.
    modules: Vec<(ModuleId, PathBuf)>,
}

impl BodyCache {
    pub(super) fn new(
        config: &ToolchainConfig,
        loaded: &LoadedProgram<'_>,
        hashes: &[ModuleHashes],
        analysis_options: impl std::fmt::Debug,
    ) -> Option<Self> {
        let directory = config.prefix.join("cache").join("body-checks");
        let compiler = rt_stamp::persistent_tool_identity(
            &std::env::current_exe().ok()?,
            &config.tool_identity_cache_root(),
        )?;
        let mut partition = blake3::Hasher::new();
        partition.update(b"dream-body-checks-v5");
        partition.update(compiler.as_bytes());
        partition.update(loaded.acc.cfg.key().as_bytes());
        partition.update(format!("{analysis_options:?}").as_bytes());
        let modules = loaded
            .graph
            .modules
            .iter()
            .zip(hashes)
            .filter(|(module, _)| !module.files.is_empty())
            .map(|(module, hashes)| {
                let mut key = partition.clone();
                key.update(&hashes.cache_key());
                (module.id, directory.join(key.finalize().to_hex().as_str()))
            })
            .collect();
        Some(Self { directory, modules })
    }

    pub(super) fn load(&mut self) -> (IndexSet<ModuleId>, IndexMap<ModuleId, TypedBodies>) {
        let mut hits = IndexSet::new();
        let mut modules = IndexMap::new();
        for (module, path) in &self.modules {
            let bodies = read_bodies(path).inspect(|_| {
                cache_budget::touch(path);
                hits.insert(*module);
            });
            modules.insert(*module, bodies.unwrap_or_default());
        }
        (hits, modules)
    }

    /// Writes the modules whose bodies `changed`, and records successful body checks of the
    /// others that have no file yet.
    pub(super) fn store(
        &self,
        with_diagnostics: &IndexSet<ModuleId>,
        typed: &IndexMap<ModuleId, TypedBodies>,
        changed: &IndexSet<ModuleId>,
    ) {
        if self.modules.is_empty() || std::fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        for (module, path) in &self.modules {
            if with_diagnostics.contains(module) || (path.is_file() && !changed.contains(module)) {
                continue;
            }
            let Some(bodies) = typed.get(module) else {
                continue;
            };
            let Ok(bytes) = serde_json::to_vec(&bodies.iter().collect::<Vec<_>>()) else {
                continue;
            };
            if bytes.len() > MAX_MODULE_BYTES {
                continue;
            }
            super::checked_file::write(path, &bytes);
        }
    }
}

const MAX_MODULE_BYTES: usize = 16 * 1024 * 1024;

fn read_bodies(path: &std::path::Path) -> Option<TypedBodies> {
    let payload = super::checked_file::read(path, MAX_MODULE_BYTES)?;
    serde_json::from_slice::<Vec<_>>(&payload)
        .ok()
        .map(|bodies| bodies.into_iter().collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn cached_float_literals_keep_their_exact_bits() {
        for value in [
            f64::from_bits(0x3E70_0000_0000_0000),
            0.1,
            f64::MIN_POSITIVE,
            5e-324,
            f64::MAX,
        ] {
            let bytes = serde_json::to_vec(&value).unwrap();
            let back: f64 = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(back.to_bits(), value.to_bits(), "{value:e}");
        }
    }
}
