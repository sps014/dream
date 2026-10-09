use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

const STAMP_HEADER: &str = "dream-build-cache-v4";

/// What a cached compile produced: either fresh output whose artifacts the caller records once its
/// own post-processing (native link, object emission) succeeds, or a verified earlier build.
#[derive(Debug)]
pub enum BuildOutcome {
    Built(Option<BuildStamp>),
    Cached(Vec<PathBuf>),
}

/// The key of a fresh build, written next to its output once every artifact exists.
#[derive(Debug)]
pub struct BuildStamp {
    path: PathBuf,
    key: blake3::Hash,
    request: blake3::Hash,
    inputs: Vec<(PathBuf, String)>,
    early: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredBuild {
    version: String,
    key: String,
    request: String,
    inputs: Vec<(PathBuf, String)>,
    early: bool,
    artifacts: Vec<(PathBuf, String)>,
}

fn input_hash(path: &Path) -> Option<String> {
    if path.is_dir() {
        let mut entries = fs::read_dir(path)
            .ok()?
            .map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let kind = entry.file_type().ok()?;
                Some((name, kind.is_dir(), kind.is_symlink()))
            })
            .collect::<Option<Vec<_>>>()?;
        entries.retain(|(name, directory, symlink)| {
            *directory
                || *symlink
                || name.ends_with(".dream")
                || matches!(name.as_str(), "dream.toml" | "dream.lock")
        });
        entries.sort();
        Some(format!(
            "dir:{}",
            blake3::hash(&serde_json::to_vec(&entries).ok()?)
        ))
    } else {
        match fs::read(path) {
            Ok(bytes) => Some(format!("file:{}", blake3::hash(&bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some("missing".into()),
            Err(_) => None,
        }
    }
}

fn verified_artifacts(stored: &StoredBuild) -> Option<Vec<PathBuf>> {
    if stored.artifacts.is_empty() {
        return None;
    }
    stored
        .artifacts
        .iter()
        .map(|(path, hash)| {
            (blake3::hash(&fs::read(path).ok()?).to_hex().as_str() == hash).then(|| path.clone())
        })
        .collect()
}

impl BuildStamp {
    pub fn store(&self, artifacts: &[PathBuf]) {
        let result = || -> Option<()> {
            if self
                .inputs
                .iter()
                .any(|(path, hash)| input_hash(path).as_ref() != Some(hash))
            {
                return None;
            }
            let artifacts = artifacts
                .iter()
                .map(|path| {
                    Some((
                        path.clone(),
                        blake3::hash(&fs::read(path).ok()?).to_hex().to_string(),
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            if artifacts.is_empty() {
                return None;
            }
            let stored = StoredBuild {
                version: STAMP_HEADER.into(),
                key: self.key.to_hex().to_string(),
                request: self.request.to_hex().to_string(),
                inputs: self.inputs.clone(),
                early: self.early,
                artifacts,
            };
            let bytes = serde_json::to_vec(&stored).ok()?;
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let partial = self.path.with_extension(format!(
                "dream-cache.{}.{}.partial",
                std::process::id(),
                nonce
            ));
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&partial)
                .ok()?;
            use std::io::Write;
            let published = file
                .write_all(&bytes)
                .and_then(|()| file.sync_all())
                .and_then(|()| fs::rename(&partial, &self.path));
            if published.is_err() {
                let _ = fs::remove_file(partial);
                return None;
            }
            Some(())
        };
        if result().is_none() {
            let _ = fs::remove_file(&self.path);
        }
    }

    pub(super) fn lookup(&self) -> Option<Vec<PathBuf>> {
        let stored: StoredBuild = serde_json::from_slice(&fs::read(&self.path).ok()?).ok()?;
        if stored.version != STAMP_HEADER || stored.key != self.key.to_hex().as_str() {
            return None;
        }
        if stored
            .inputs
            .iter()
            .any(|(path, hash)| input_hash(path).as_ref() != Some(hash))
        {
            return None;
        }
        verified_artifacts(&stored)
    }
}

impl Compiler {
    pub(super) fn early_build_lookup(
        &self,
        main_file_path: &str,
        out_path: &str,
    ) -> Option<Vec<PathBuf>> {
        if self.virtual_entry.is_some()
            || self.emit_mir.is_some()
            || self.opt_remarks
            || !self.toolchain_config.compiler_environment.is_empty()
        {
            return None;
        }
        let stored: StoredBuild = serde_json::from_slice(
            &fs::read(Path::new(out_path).with_extension("dream-cache")).ok()?,
        )
        .ok()?;
        if stored.version != STAMP_HEADER || !stored.early {
            return None;
        }
        let key = self.request_key(self.build_cache.as_deref()?, main_file_path, out_path)?;
        if stored.request != key.to_hex().as_str()
            || stored
                .inputs
                .iter()
                .any(|(path, hash)| input_hash(path).as_ref() != Some(hash))
        {
            return None;
        }
        verified_artifacts(&stored)
    }

    fn request_key(
        &self,
        link_key: &str,
        main_file_path: &str,
        out_path: &str,
    ) -> Option<blake3::Hash> {
        let mut hash = blake3::Hasher::new();
        let mut field = |label: &str, value: &[u8]| {
            hash.update(&(label.len() as u64).to_le_bytes());
            hash.update(label.as_bytes());
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        };
        field("header", STAMP_HEADER.as_bytes());
        let exe = std::env::current_exe().ok()?;
        field(
            "compiler",
            crate::driver::rt_stamp::tool_identity(&exe)?.as_bytes(),
        );
        field(
            "runtime",
            crate::driver::rt_stamp::content_fingerprint(crate::driver::rt_stamp::files_under(
                &self.toolchain_config.runtime_c,
            ))
            .as_bytes(),
        );
        let options = format!(
            "{:?}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{}",
            self.target,
            self.profile.is_debug(),
            self.debug_info,
            self.optimize,
            self.generator_stage,
            self.runtimes,
            self.compile_targets,
            self.crate_type,
            self.opt_ir,
            self.output_kind,
            self.raw_ir_intermediate,
        );
        field("options", options.as_bytes());
        field("link", link_key.as_bytes());
        let toolchain = &self.toolchain_config;
        field("toolchain", toolchain.fingerprint().as_bytes());
        #[cfg(feature = "native")]
        {
            let spec = self.target.spec();
            let tools = crate::execution::llvm::resolve_llvm(toolchain).ok()?;
            if spec.is_apple() {
                field(
                    "resolved-sdk",
                    format!(
                        "{:?}",
                        crate::execution::llvm::runtime::sysroot_args(toolchain, spec)
                    )
                    .as_bytes(),
                );
            }
            let mut paths = [
                "opt",
                "llc",
                "llvm-link",
                "llvm-dis",
                "clang",
                "llvm-ar",
                "llvm-rc",
            ]
            .map(|name| tools.tool(name))
            .to_vec();
            if !spec.capabilities.linear_memory {
                if spec.is_windows() {
                    paths.extend(crate::execution::native::cc::installed_zig(toolchain));
                }
                paths.push(
                    crate::execution::native::cc::resolve_existing_target_cc(toolchain, spec)
                        .ok()?
                        .path()
                        .to_path_buf(),
                );
                let directories = if spec.can_link_on_host() {
                    toolchain.host_library_dirs()
                } else {
                    vec![toolchain.targets.join(spec.triple.to_string()).join("lib")]
                };
                for dir in directories {
                    for capability in dream_abi::host_capability::HostCapability::ALL {
                        paths.push(dir.join(capability.library_name(spec)));
                        if spec.is_windows() {
                            paths.push(dir.join(capability.import_library_name(spec)));
                        }
                    }
                }
            }
            field(
                "resolved-tools",
                format!(
                    "{:?}",
                    paths
                        .iter()
                        .map(|path| (path, crate::driver::rt_stamp::tool_identity(path)))
                        .collect::<Vec<_>>()
                )
                .as_bytes(),
            );
        }
        let env: BTreeMap<String, String> = std::env::vars()
            .filter(|(name, _)| name.starts_with("DREAM_"))
            .collect();
        field("env", format!("{env:?}").as_bytes());
        field(
            "cwd",
            std::env::current_dir().ok()?.as_os_str().as_encoded_bytes(),
        );
        field("entry", main_file_path.as_bytes());
        field("output", out_path.as_bytes());
        let manifest =
            crate::driver::project_manifest::find_project_root_from(Path::new(main_file_path))
                .and_then(|root| {
                    fs::read(root.join(crate::driver::project_manifest::MANIFEST_FILE_NAME)).ok()
                })
                .unwrap_or_default();
        field("manifest", &manifest);
        Some(hash.finalize())
    }

    /// The stamp for this build, or `None` when an input the key cannot see takes part: native C
    /// or C++ sets compile files outside the module graph, and MIR dumps are side outputs.
    pub(super) fn build_stamp(
        &self,
        link_key: &str,
        loaded: &load::LoadedProgram<'_>,
        main_file_path: &str,
        out_path: &str,
    ) -> Option<BuildStamp> {
        if self.opt_remarks
            || self.emit_mir.is_some()
            || !self.toolchain_config.compiler_environment.is_empty()
            || !loaded.native_graph.sets.is_empty()
            || !loaded.cpp_bridge.is_empty()
            || loaded.acc.untracked_generator_inputs
            || !loaded.acc.aliased_imports.is_empty()
        {
            return None;
        }
        let request = self.request_key(link_key, main_file_path, out_path)?;
        let mut hash = blake3::Hasher::new();
        hash.update(request.as_bytes());
        let mut field = |label: &str, value: &[u8]| {
            hash.update(label.as_bytes());
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        };
        for file in &loaded.graph.files {
            field("file", file.path.as_bytes());
            field("source", file.source.as_bytes());
        }
        for module in &loaded.graph.modules {
            field("module", &module.cache_key());
        }
        let mut inputs = loaded.acc.resolution_inputs.clone();
        for file in &loaded.graph.files {
            let path = PathBuf::from(&file.path);
            if path.is_file() {
                inputs.insert(path);
            }
        }
        for ancestor in Path::new(main_file_path).parent()?.ancestors() {
            inputs.insert(ancestor.join("dream.toml"));
            inputs.insert(ancestor.join("dream.lock"));
            if ancestor.join("dream.toml").is_file() {
                break;
            }
        }
        let inputs = inputs
            .into_iter()
            .map(|path| Some((path.clone(), input_hash(&path)?)))
            .collect::<Option<Vec<_>>>()?;
        Some(BuildStamp {
            path: Path::new(out_path).with_extension("dream-cache"),
            key: hash.finalize(),
            request,
            inputs,
            early: self.virtual_entry.is_none()
                && self.toolchain_config.compiler_environment.is_empty(),
        })
    }
}
