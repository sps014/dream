//! Executes `@generator` bodies that take a single `GenContext` parameter: compiles a small
//! auto-generated Dream entry that imports the generator's own module, calls the user's
//! function with a `GenContext` loaded from a snapshot, then flushes `ctx.finish()`. Reuses the
//! same snapshot format and GenHost stdout protocol helpers in `syntax_gen`.

use super::context::GeneratorContext;
use super::registration::RegisteredGenerator;
use super::syntax::SyntaxNodeId;
use dream_diagnostics::DiagnosticBag;
use std::collections::HashSet;

#[cfg(feature = "native")]
use super::syntax_gen::{build_snapshot, parse_harness_output, HarnessError};
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};
#[cfg(feature = "native")]
use std::sync::Mutex;

#[cfg(feature = "native")]
const SNAPSHOT_ENV: &str = "DREAM_SYNTAX_GEN_SNAPSHOT";

/// Runs every registered `@generator(ctx: GenContext)` body that claims syntax blocks. Returns
/// the set of generator names it handled.
pub fn expand_context_generators(
    ctx: &mut GeneratorContext,
    diagnostics: &mut DiagnosticBag,
) -> HashSet<String> {
    let gens: Vec<RegisteredGenerator> = ctx
        .registered
        .iter()
        .filter(|g| g.has_context_body)
        .cloned()
        .collect();
    let mut handled = HashSet::new();
    if gens.is_empty() {
        return handled;
    }

    for gen in gens {
        let mut site_ids: Vec<SyntaxNodeId> = Vec::new();
        let mut seen: HashSet<u32> = HashSet::new();
        for name in &gen.syntax_blocks {
            for id in ctx.syntax_blocks(name) {
                if seen.insert(id.0) {
                    site_ids.push(id);
                }
            }
        }
        handled.insert(gen.name.clone());

        #[cfg(not(feature = "native"))]
        {
            let _ = &site_ids;
            diagnostics.report_error(
                "@generator(ctx: GenContext) execution requires the native compiler feature (wasmtime host)"
                    .to_string(),
                None,
            );
            continue;
        }

        #[cfg(feature = "native")]
        {
            let snapshot = build_snapshot(ctx, &site_ids);
            match run_context_body(&gen, &snapshot) {
                Ok(output) => {
                    for (id, source) in output.replacements {
                        ctx.replace(id, source);
                    }
                    for (type_name, body) in output.extend_emits {
                        ctx.emit_extend(type_name, body);
                    }
                    for (path, source) in output.file_emits {
                        ctx.emit_file(path, source);
                    }
                }
                Err(err) => match err {
                    HarnessError::Site { id, message } => {
                        ctx.error(id, message);
                    }
                    HarnessError::General(message) => {
                        diagnostics.report_error(message, None);
                    }
                },
            }
        }
    }
    handled
}

#[cfg(feature = "native")]
fn run_context_body(
    gen: &RegisteredGenerator,
    snapshot: &str,
) -> Result<super::syntax_gen::HarnessOutput, HarnessError> {
    static SNAPSHOT_GUARD: Mutex<()> = Mutex::new(());
    let _guard = SNAPSHOT_GUARD.lock().unwrap_or_else(|e| e.into_inner());

    let gen_path = Path::new(&gen.file_path);
    let Some(dir) = gen_path.parent() else {
        return Err(HarnessError::General(format!(
            "generator '{}': cannot resolve directory for '{}'",
            gen.name, gen.file_path
        )));
    };
    let Some(stem) = gen_path.file_stem().and_then(|s| s.to_str()) else {
        return Err(HarnessError::General(format!(
            "generator '{}': cannot resolve module name for '{}'",
            gen.name, gen.file_path
        )));
    };

    let harness_source = format!(
        "import system;\nimport system.io;\nimport system.collections;\nimport system.json;\nimport system.codegen;\nimport {stem};\n\nasync fun main(): void {{\n    let path = System.env_or(\"{env}\", \"\");\n    let ctx_res = await GenContext.from_snapshot(path);\n    switch (ctx_res) {{\n        Ok(gen_ctx) => {{\n            {func}(gen_ctx);\n            gen_ctx.finish();\n        }}\n        Err(e) => {{\n            System.println(GenHost.err_marker());\n            System.println(e);\n        }}\n    }}\n}}\n",
        env = SNAPSHOT_ENV,
        stem = stem,
        func = gen.name,
    );

    let temp =
        write_temp_harness(dir, &gen.name, &harness_source).map_err(HarnessError::General)?;

    let wat_path = compile_harness(&temp.path).map_err(HarnessError::General)?;
    let snap_file = write_snapshot_tempfile(&gen.name, snapshot).map_err(HarnessError::General)?;

    std::env::set_var(SNAPSHOT_ENV, snap_file.to_string_lossy().as_ref());
    let wat_path_str = wat_path.to_string_lossy().into_owned();
    let output =
        crate::execution::wasm_runner::execute_wasm_capturing(&wat_path_str).map_err(|e| {
            HarnessError::General(format!(
                "generator '{}': failed to run generated harness: {e}",
                gen.name
            ))
        });
    std::env::remove_var(SNAPSHOT_ENV);
    let _ = std::fs::remove_file(&snap_file);
    let _ = std::fs::remove_file(&wat_path);

    parse_harness_output(&output?)
}

#[cfg(feature = "native")]
struct TempHarnessFile {
    path: PathBuf,
}

#[cfg(feature = "native")]
impl Drop for TempHarnessFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(feature = "native")]
fn write_temp_harness(dir: &Path, gen_name: &str, source: &str) -> Result<TempHarnessFile, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!(
        ".dream_gen_autoharness_{}_{}_{}.dream",
        gen_name,
        std::process::id(),
        unique
    ));
    std::fs::write(&path, source)
        .map_err(|e| format!("generator '{gen_name}': write auto-harness: {e}"))?;
    Ok(TempHarnessFile { path })
}

#[cfg(feature = "native")]
fn write_snapshot_tempfile(gen_name: &str, snapshot: &str) -> Result<PathBuf, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dream-ctx-gen-snap-{}-{}.json",
        std::process::id(),
        unique
    ));
    std::fs::write(&path, snapshot)
        .map_err(|e| format!("generator '{gen_name}': write snapshot: {e}"))?;
    Ok(path)
}

#[cfg(feature = "native")]
fn compile_harness(src_path: &Path) -> Result<PathBuf, String> {
    let mut wat_path = std::env::temp_dir();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    wat_path.push(format!(
        "dream-ctx-gen-{}-{}.wat",
        std::process::id(),
        unique
    ));
    let compiler = crate::driver::compiler::Compiler::new(crate::driver::compiler::Target::Wasm)
        .with_skip_generators(true)
        .with_release(true);
    let src = src_path
        .to_str()
        .ok_or_else(|| "generator: non-UTF-8 auto-harness path".to_string())?
        .to_string();
    let out = wat_path
        .to_str()
        .ok_or_else(|| "generator: non-UTF-8 wat path".to_string())?
        .to_string();
    compiler
        .compile(&src, &out)
        .map_err(|e| format!("generator: failed to compile auto-harness: {e:?}"))?;
    Ok(wat_path)
}
