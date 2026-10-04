use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

const STAMP_HEADER: &str = "dream-build-cache-v1";

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
}

impl BuildStamp {
    /// Records `artifacts` with their content hashes. A missing artifact leaves no stamp, so the
    /// next build recompiles instead of reusing an incomplete set.
    pub fn store(&self, artifacts: &[PathBuf]) {
        let mut text = format!("{STAMP_HEADER} {}\n", self.key.to_hex());
        for artifact in artifacts {
            let Ok(bytes) = fs::read(artifact) else {
                let _ = fs::remove_file(&self.path);
                return;
            };
            text.push_str(&format!("{} {}\n", blake3::hash(&bytes).to_hex(), artifact.display()));
        }
        let partial = self.path.with_extension("dream-cache.partial");
        if fs::write(&partial, text).is_ok() && fs::rename(&partial, &self.path).is_err() {
            let _ = fs::remove_file(&partial);
        }
    }

    /// The recorded artifacts when the stamp holds this key and every artifact is byte-identical
    /// to what that build wrote.
    pub(super) fn lookup(&self) -> Option<Vec<PathBuf>> {
        let text = fs::read_to_string(&self.path).ok()?;
        let mut lines = text.lines();
        let (header, key) = lines.next()?.split_once(' ')?;
        if header != STAMP_HEADER || key != self.key.to_hex().as_str() {
            return None;
        }
        lines
            .map(|line| {
                let (hash, path) = line.split_once(' ')?;
                let path = PathBuf::from(path);
                let bytes = fs::read(&path).ok()?;
                (blake3::hash(&bytes).to_hex().as_str() == hash).then_some(path)
            })
            .collect()
    }
}

impl Compiler {
    /// The stamp for this build, or `None` when an input the key cannot see takes part: native C
    /// or C++ sets compile files outside the module graph, and MIR dumps are side outputs.
    pub(super) fn build_stamp(
        &self,
        link_key: &str,
        loaded: &load::LoadedProgram<'_>,
        main_file_path: &str,
        out_path: &str,
    ) -> Option<BuildStamp> {
        if self.emit_mir.is_some()
            || !loaded.native_graph.sets.is_empty()
            || !loaded.cpp_bridge.is_empty()
        {
            return None;
        }
        let mut hash = blake3::Hasher::new();
        let mut field = |label: &str, value: &[u8]| {
            hash.update(&(label.len() as u64).to_le_bytes());
            hash.update(label.as_bytes());
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        };
        field("header", STAMP_HEADER.as_bytes());
        let exe = std::env::current_exe().ok()?;
        field("compiler", crate::driver::rt_stamp::fingerprint(vec![exe]).as_bytes());
        field(
            "runtime",
            crate::driver::rt_stamp::fingerprint(files_under(&self.toolchain_config.runtime_c))
                .as_bytes(),
        );
        let options = format!(
            "{:?}|{}|{}|{:?}|{}|{:?}|{:?}|{:?}|{}",
            self.target,
            self.debug,
            self.debug_info,
            self.optimize,
            self.skip_generators,
            self.runtimes,
            self.compile_targets,
            self.crate_type,
            self.opt_ir,
        );
        field("options", options.as_bytes());
        field("link", link_key.as_bytes());
        let toolchain = &self.toolchain_config;
        field(
            "toolchain",
            format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                toolchain.llvm,
                toolchain.toolchains,
                toolchain.cc,
                toolchain.cxx,
                toolchain.zig,
                toolchain.native_sanitize,
                toolchain.sdkroot,
                toolchain.path,
            )
            .as_bytes(),
        );
        let env: BTreeMap<String, String> = std::env::vars()
            .filter(|(name, _)| name.starts_with("DREAM_"))
            .collect();
        field("env", format!("{env:?}").as_bytes());
        field("entry", main_file_path.as_bytes());
        field("output", out_path.as_bytes());
        let manifest = crate::driver::project_manifest::find_project_root_from(Path::new(
            main_file_path,
        ))
        .and_then(|root| {
            fs::read(root.join(crate::driver::project_manifest::MANIFEST_FILE_NAME)).ok()
        })
        .unwrap_or_default();
        field("manifest", &manifest);
        for file in &loaded.graph.files {
            field("file", file.path.as_bytes());
            field("source", file.source.as_bytes());
        }
        for module in &loaded.graph.modules {
            field("module", &module.cache_key());
        }
        Some(BuildStamp {
            path: Path::new(out_path).with_extension("dream-cache"),
            key: hash.finalize(),
        })
    }
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(_) => files.push(path),
                Err(_) => {}
            }
        }
    }
    files
}
