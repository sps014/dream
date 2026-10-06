//! Cached generator executables: one linked, debuggable binary per generator module (a user
//! file or a std generator package), dispatching on `--generator <name>`. Entries live under
//! `<generator cache>/exe/<key>/` and are built at most once at a time per key (a file lock).
//! They are built in place, never moved: the macOS debug map names `gen.o` by absolute path.
//! `meta.json` is written last, so an interrupted build is never mistaken for a finished one.

use super::registry::RegisteredGenerator;
use super::stage::GeneratorStage;
use crate::driver::toolchain::ToolchainConfig;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
#[cfg(feature = "native")]
use std::sync::Arc;

/// Generators sharing one executable.
#[derive(Debug, Clone)]
pub struct ExeGroup<'g> {
    pub gens: Vec<&'g RegisteredGenerator>,
}

impl ExeGroup<'_> {
    fn first(&self) -> &RegisteredGenerator {
        self.gens[0]
    }

    pub fn is_std(&self) -> bool {
        self.first().is_std()
    }

    /// The import that brings every generator of the group into scope.
    fn import(&self) -> String {
        let registered = self.first();
        match registered.std_package {
            Some(package) => package.to_string(),
            None => Path::new(&registered.file)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    /// Virtual entry path of the harness: beside a user generator (so its import resolves), or
    /// in the cache for a std package.
    fn entry_path(&self, cache_root: &Path) -> PathBuf {
        let registered = self.first();
        match registered.std_package {
            Some(package) => cache_root.join("harness").join(format!("{package}.dream")),
            None => Path::new(&registered.file)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("__dream_generators__.dream"),
        }
    }
}

/// Groups generators by declaring module, in first-generator name order.
pub fn group<'g>(gens: &[&'g RegisteredGenerator]) -> Vec<ExeGroup<'g>> {
    let mut groups: Vec<ExeGroup<'g>> = Vec::new();
    for &registered in gens {
        let module_key = |g: &RegisteredGenerator| match g.std_package {
            Some(p) => p.to_string(),
            None => g.file.clone(),
        };
        match groups
            .iter_mut()
            .find(|grp| module_key(grp.first()) == module_key(registered))
        {
            Some(grp) => grp.gens.push(registered),
            None => groups.push(ExeGroup { gens: vec![registered] }),
        }
    }
    groups
}

fn harness_source(group: &ExeGroup<'_>) -> String {
    let mut src = String::new();
    let _ = writeln!(
        src,
        "import system;\nimport system.io;\nimport system.collections;\nimport system.codegen;\nimport {};\n",
        group.import()
    );
    src.push_str("async fun main(): int {\n");
    src.push_str("    let loaded = GenContext.load(System.args).await;\n");
    src.push_str("    switch (loaded) {\n");
    src.push_str("        Ok(gen_ctx) => {\n");
    src.push_str("            let name = gen_ctx.generator_name;\n");
    for (i, registered) in group.gens.iter().enumerate() {
        let kw = if i == 0 { "if" } else { "} else if" };
        let _ = writeln!(src, "            {kw} name == \"{}\" {{", registered.name);
        // A sink `ctx: GenContext` parameter moves `arg`, so `gen_ctx` still owns the
        // context for `finish()`.
        let _ = writeln!(
            src,
            "                let arg = gen_ctx;\n                {}(arg);",
            registered.name
        );
    }
    src.push_str("            } else {\n");
    src.push_str("                gen_ctx.error(\"no generator named '\" + name + \"' in this executable\");\n");
    src.push_str("            }\n");
    src.push_str("            return gen_ctx.finish().await;\n");
    src.push_str("        }\n");
    src.push_str("        Err(e) => {\n");
    src.push_str("            System.println(e);\n");
    src.push_str("            return 2;\n");
    src.push_str("        }\n");
    src.push_str("    }\n");
    src.push_str("}\n");
    src
}

/// A generator executable's cache identity and where it lives once built.
#[derive(Debug, Clone)]
pub struct ExePlan {
    pub key: String,
    pub dir: PathBuf,
    pub entry: String,
    pub harness: String,
    pub stage: GeneratorStage,
    pub generators: Vec<String>,
    /// `(label, value or digest)` pairs the key hashes, in hash order; `--explain` diffs them
    /// against the last build of the same entry.
    pub components: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct GenExe {
    pub ll: PathBuf,
    pub bin: PathBuf,
}

/// User sources the executable is built from (the generator file and its user imports); std
/// sources are part of the compiler identity.
fn user_sources(entry: &str, harness: &str) -> BTreeMap<String, String> {
    let arena = bumpalo::Bump::new();
    let mut acc = crate::driver::source_loader::ProgramAccumulator::default();
    let mut scratch = dream_diagnostics::DiagnosticBag::new(None);
    let _ = crate::driver::source_loader::parse_source_recursive(
        entry.to_string(),
        harness.to_string(),
        &mut acc,
        &arena,
        &mut scratch,
    );
    acc.file_contents
        .into_iter()
        .filter(|(path, _)| path != entry && !dream_stdlib::is_std_source(path))
        .collect()
}

pub fn plan(config: &ToolchainConfig, identity: &str, group: &ExeGroup<'_>) -> ExePlan {
    let cache_root = config.generator_cache_root();
    let harness = harness_source(group);
    let entry = group.entry_path(&cache_root).to_string_lossy().into_owned();
    let stage = GeneratorStage::for_generator_build(group.is_std());
    let mut components: Vec<(String, String)> = vec![
        ("compiler".to_string(), digest(identity)),
        ("stage".to_string(), format!("{stage:?}")),
        ("entry".to_string(), entry.clone()),
        ("harness".to_string(), digest(&harness)),
    ];
    if !group.is_std() {
        for (path, text) in user_sources(&entry, &harness) {
            components.push((format!("source {path}"), digest(&text)));
        }
    }
    let mut hash = blake3::Hasher::new();
    for (label, value) in &components {
        hash.update(&(label.len() as u64).to_le_bytes());
        hash.update(label.as_bytes());
        hash.update(&(value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let key = hash.finalize().to_hex().to_string();
    ExePlan {
        dir: cache_root.join("exe").join(&key),
        key,
        entry,
        harness,
        stage,
        generators: group.gens.iter().map(|g| g.name.clone()).collect(),
        components,
    }
}

fn digest(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..16].to_string()
}

impl ExePlan {
    pub fn exe(&self) -> GenExe {
        GenExe {
            ll: self.dir.join("gen.ll"),
            bin: self.dir.join(bin_name()),
        }
    }

    pub fn is_built(&self) -> bool {
        self.dir.join("meta.json").is_file() && self.exe().bin.is_file()
    }

    fn last_path(&self) -> Option<PathBuf> {
        let exe_root = self.dir.parent()?;
        let cache_root = exe_root.parent()?;
        Some(
            cache_root
                .join("last")
                .join(format!("{}.json", digest(&self.entry))),
        )
    }

    /// The components of the executable last used for this entry, if any.
    pub fn last_components(&self) -> Option<Vec<(String, String)>> {
        let text = std::fs::read_to_string(self.last_path()?).ok()?;
        serde_json::from_str(&text).ok()
    }

    #[cfg(feature = "native")]
    fn record_last(&self) {
        let Some(path) = self.last_path() else { return };
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        if let Ok(text) = serde_json::to_string(&self.components) {
            let _ = std::fs::write(path, text);
        }
    }
}

fn bin_name() -> &'static str {
    if cfg!(windows) {
        "gen.exe"
    } else {
        "gen.bin"
    }
}

#[cfg(feature = "native")]
pub fn ensure_built(
    config: &Arc<ToolchainConfig>,
    plan: &ExePlan,
) -> Result<(GenExe, bool), String> {
    let built = build_once(config, plan);
    if built.is_ok() {
        plan.record_last();
    }
    built
}

#[cfg(feature = "native")]
fn build_once(config: &Arc<ToolchainConfig>, plan: &ExePlan) -> Result<(GenExe, bool), String> {
    if plan.is_built() {
        return Ok((plan.exe(), false));
    }
    let Some(parent) = plan.dir.parent() else {
        return Err("generator cache directory has no parent".to_string());
    };
    std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    let lock_path = parent.join(format!("{}.lock", plan.key));
    let lock = std::fs::File::create(&lock_path)
        .map_err(|e| format!("create {}: {e}", lock_path.display()))?;
    lock.lock()
        .map_err(|e| format!("lock {}: {e}", lock_path.display()))?;
    if plan.is_built() {
        return Ok((plan.exe(), false));
    }
    let _ = std::fs::remove_dir_all(&plan.dir);
    std::fs::create_dir_all(&plan.dir)
        .map_err(|e| format!("create {}: {e}", plan.dir.display()))?;
    if let Err(e) = build_into(config, plan, &plan.dir) {
        let _ = std::fs::remove_dir_all(&plan.dir);
        return Err(e);
    }
    Ok((plan.exe(), true))
}

#[cfg(feature = "native")]
fn build_into(config: &Arc<ToolchainConfig>, plan: &ExePlan, dir: &Path) -> Result<(), String> {
    use crate::driver::compiler::Compiler;
    use crate::driver::wasm_opt::OptLevel;

    let ll = dir.join("gen.ll");
    let ll_str = ll.to_string_lossy().into_owned();
    let compiler =
        Compiler::new_with_toolchain_config(dream_mir::backend::Target::native(), config.clone())
            .with_generator_stage(plan.stage)
            .with_virtual_entry(plan.harness.clone())
            .with_render_diagnostics(false)
            .with_release(true)
            .with_debug_info(true)
            .with_optimize(Some(OptLevel::O0));
    compiler
        .compile(&plan.entry, &ll_str)
        .map_err(|e| match e.diagnostic_text() {
            Some(text) => format!("generator executable failed to compile:\n{text}"),
            None => format!("generator executable failed to compile: {e}"),
        })?;
    let bin = crate::execution::llvm::compile_llvm(
        config,
        &ll,
        crate::execution::llvm::NativeBuildOptions {
            target: dream_abi::target::TargetSpec::host(),
            opt_ll: None,
            opt: OptLevel::O0,
            debug: true,
            pgo: &crate::execution::native::Pgo::Off,
            icon: None,
            relocatable: false,
            output_kind: crate::driver::output::OutputKind::Executable,
        },
    )
    .map_err(|e| format!("generator executable failed to link: {e}"))?;
    let want = dir.join(bin_name());
    if bin != want {
        std::fs::rename(&bin, &want).map_err(|e| format!("move {}: {e}", bin.display()))?;
    }
    let meta = serde_json::json!({
        "key": plan.key,
        "entry": plan.entry,
        "generators": plan.generators,
        "stage": format!("{:?}", plan.stage),
    });
    std::fs::write(dir.join("meta.json"), meta.to_string())
        .map_err(|e| format!("write meta.json: {e}"))?;
    Ok(())
}
