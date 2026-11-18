//! Persists successful body checks and reusable typed bodies by source module.

use super::load::LoadedProgram;
use crate::driver::{cache_budget, rt_stamp, toolchain::ToolchainConfig};
use dream_sema::{analyzer::TypedBodies, module_graph::ModuleHashes};
use dream_types::ModuleId;
use indexmap::{IndexMap, IndexSet};
use std::path::PathBuf;

pub(super) struct BodyCache {
    directory: PathBuf,
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
        partition.update(b"dream-body-checks-v3");
        partition.update(compiler.as_bytes());
        partition.update(loaded.acc.cfg.key().as_bytes());
        partition.update(format!("{analysis_options:?}").as_bytes());
        let modules = loaded
            .graph
            .modules
            .iter()
            .zip(hashes)
            .filter(|(module, _)| {
                !module.path.is_empty()
                    && module.files.iter().any(|&index| {
                        let file = &loaded.graph.files[index];
                        !file.path.starts_with('<')
                            && file.program.functions.iter().any(|function| {
                                !function.is_extern && function.generic_parameters.is_none()
                            })
                    })
            })
            .map(|(module, hashes)| {
                let mut key = partition.clone();
                key.update(&hashes.cache_key());
                (module.id, directory.join(key.finalize().to_hex().as_str()))
            })
            .collect();
        Some(Self { directory, modules })
    }

    pub(super) fn load(&self) -> (IndexSet<ModuleId>, IndexMap<ModuleId, TypedBodies>) {
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

    pub(super) fn store(
        &self,
        with_diagnostics: &IndexSet<ModuleId>,
        typed: &IndexMap<ModuleId, TypedBodies>,
    ) {
        if self.modules.is_empty() || std::fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        for (module, path) in &self.modules {
            if with_diagnostics.contains(module) || path.is_file() {
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
            let checksum = blake3::hash(&bytes);
            let record = [checksum.as_bytes().as_slice(), &bytes].concat();
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let partial = path.with_extension(format!("{}.{nonce}.partial", std::process::id()));
            if std::fs::write(&partial, record).is_err() || std::fs::rename(&partial, path).is_err()
            {
                let _ = std::fs::remove_file(partial);
            }
        }
    }
}

const MAX_MODULE_BYTES: usize = 16 * 1024 * 1024;

fn read_bodies(path: &std::path::Path) -> Option<TypedBodies> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take((MAX_MODULE_BYTES + 33) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let (checksum, payload) = bytes.split_at_checked(32)?;
    if payload.len() > MAX_MODULE_BYTES || blake3::hash(payload).as_bytes() != checksum {
        return None;
    }
    serde_json::from_slice::<Vec<_>>(payload)
        .ok()
        .map(|bodies| bodies.into_iter().collect())
}
