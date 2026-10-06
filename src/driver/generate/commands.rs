//! `dream generate` subcommands and `dream debug-adapter --generator`: everything runs on a
//! [`GenInspection`], so none of them merges generated code or compiles the program itself.

use super::exe::{ensure_built, ExePlan, GenExe};
use super::inspect::{GenInspection, InspectedGenerator};
use super::model::{GenResult, Output};
use super::run::{write_capture, Capture, CAPTURE_FILE, SNAPSHOT_FILE};
use crate::driver::toolchain::ToolchainConfig;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn find<'i>(insp: &'i GenInspection, name: &str) -> Result<&'i InspectedGenerator, String> {
    insp.find(name).ok_or_else(|| {
        let known: Vec<&str> = insp
            .generators
            .iter()
            .map(|g| g.registered.name.as_str())
            .collect();
        if known.is_empty() {
            format!("no generator named '{name}': this program registers no generators")
        } else {
            format!(
                "no generator named '{name}' (registered: {})",
                known.join(", ")
            )
        }
    })
}

fn snapshot_of(g: &InspectedGenerator) -> Result<&str, String> {
    g.snapshot_json.as_deref().ok_or_else(|| {
        format!(
            "generator '{}' has nothing to do in this program (no trigger matches)",
            g.registered.name
        )
    })
}

fn timeout_of(g: &InspectedGenerator) -> u64 {
    g.registered
        .entry
        .as_ref()
        .and_then(|e| e.timeout_secs)
        .unwrap_or(super::run::DEFAULT_TIMEOUT_SECS)
}

fn triggers(g: &InspectedGenerator) -> Vec<String> {
    let mut out: Vec<String> = g
        .registered
        .attribute_triggers
        .iter()
        .map(|t| format!("@on_attribute({t})"))
        .collect();
    out.extend(
        g.registered
            .call_triggers
            .iter()
            .map(|t| format!("@on_call({})", t.id)),
    );
    if g.registered.syntax_block {
        out.push("@syntax_block".to_string());
    }
    out
}

/// `--list`: every registered generator with its triggers, whether it fires, and its inputs.
pub fn list(insp: &GenInspection) -> String {
    let mut out = String::new();
    if insp.generators.is_empty() {
        out.push_str("no generators registered for this program\n");
        return out;
    }
    for g in &insp.generators {
        let _ = writeln!(
            out,
            "{}{}  ({})",
            g.registered.name,
            if g.registered.incremental {
                " @incremental"
            } else {
                ""
            },
            g.registered.file
        );
        let _ = writeln!(out, "  triggers: {}", triggers(g).join(", "));
        if let Some(options) = g
            .registered
            .entry
            .as_ref()
            .map(|e| &e.options)
            .filter(|o| !o.is_empty())
        {
            let _ = writeln!(
                out,
                "  options: {}",
                serde_json::to_string(options).unwrap_or_default()
            );
        }
        let _ = writeln!(
            out,
            "  {}: {} syntax block(s), {} call site(s)",
            if g.applicable { "runs" } else { "skipped" },
            g.sites,
            g.calls
        );
        let _ = writeln!(
            out,
            "  executable: {} ({})",
            &g.plan.key[..16],
            if g.plan.is_built() {
                "cached"
            } else {
                "not built"
            }
        );
    }
    out
}

/// `--explain <gen>`: the executable and result cache keys, component by component, against
/// the executable last used for the same entry.
pub fn explain(
    config: &ToolchainConfig,
    insp: &GenInspection,
    name: &str,
) -> Result<String, String> {
    let g = find(insp, name)?;
    let mut out = String::new();
    let _ = writeln!(out, "generator '{}' ({})", g.registered.name, g.registered.file);
    let _ = writeln!(
        out,
        "executable {} — {}",
        &g.plan.key[..16],
        if g.plan.is_built() {
            "cached"
        } else {
            "will be built"
        }
    );
    let last = g.plan.last_components();
    for (label, value) in &g.plan.components {
        let status = match &last {
            None => "",
            Some(prev) => match prev.iter().find(|(l, _)| l == label) {
                Some((_, v)) if v == value => "",
                Some(_) => "  (changed)",
                None => "  (new)",
            },
        };
        let _ = writeln!(out, "  {label}: {value}{status}");
    }
    if let Some(prev) = &last {
        for (label, _) in prev {
            if !g.plan.components.iter().any(|(l, _)| l == label) {
                let _ = writeln!(out, "  {label}: (removed)");
            }
        }
    } else {
        let _ = writeln!(
            out,
            "  (no previous build of this entry to compare against)"
        );
    }
    match (&g.result_key, g.applicable) {
        (_, false) => {
            let _ = writeln!(out, "result: skipped, no trigger matches");
        }
        (Some(key), true) => {
            let hit = super::incremental::load(&config.generator_cache_root(), key).is_some();
            let _ = writeln!(
                out,
                "result {} — {} (exe key + snapshot)",
                &key[..16],
                if hit {
                    "cached, replays"
                } else {
                    "not cached, runs"
                }
            );
        }
        (None, true) => {
            let _ = writeln!(out, "result: runs every build (not @incremental)");
        }
    }
    Ok(out)
}

/// The cached executable for `plan`, building it when needed.
pub fn executable(config: &Arc<ToolchainConfig>, plan: &ExePlan) -> Result<GenExe, String> {
    ensure_built(config, plan).map(|(exe, _)| exe)
}

/// A one-line-per-item summary of a generator result.
pub fn summarize(result: &GenResult) -> String {
    let mut out = String::new();
    for output in &result.outputs {
        match output {
            Output::File { path, source } => {
                let _ = writeln!(out, "file {path} ({} lines)", source.lines().count());
            }
            Output::Replace { site, source } => {
                let _ = writeln!(out, "replace {site} ({} bytes)", source.len());
            }
        }
    }
    for d in &result.diagnostics {
        let at = if d.target.is_empty() {
            String::new()
        } else {
            format!(" [{}]", d.target)
        };
        let _ = writeln!(out, "{}{at}: {}", d.severity, d.message);
    }
    for log in &result.logs {
        let _ = writeln!(out, "log: {log}");
    }
    if out.is_empty() {
        out.push_str("no outputs, diagnostics or logs\n");
    }
    out
}

/// The default `--capture` directory: `<project or entry dir>/target/generators/<gen>`.
pub fn default_capture_dir(entry: &str, generator: &str) -> PathBuf {
    let entry_path = Path::new(entry);
    let root = super::manifest::find_project_root(entry).unwrap_or_else(|| {
        entry_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    });
    root.join("target").join("generators").join(generator)
}

/// `--capture <gen>`: builds the executable, runs the generator once, and writes the snapshot,
/// its result and a `capture.json` into `dir`.
pub fn capture(
    config: &Arc<ToolchainConfig>,
    insp: &GenInspection,
    name: &str,
    dir: &Path,
) -> Result<String, String> {
    let g = find(insp, name)?;
    let snapshot = snapshot_of(g)?;
    let exe = executable(config, &g.plan)?;
    write_capture(dir, &exe, name, snapshot, timeout_of(g))?;
    let result = super::run::run(config, &exe, name, snapshot, timeout_of(g))?;
    let path = dir.join(RESULT_FILE);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(format!(
        "captured '{name}' into {}\n{}",
        dir.display(),
        summarize(&result)
    ))
}

const RESULT_FILE: &str = "result.json";

/// Whether `path` is a directory written by `--capture` (or kept from a failed run).
pub fn is_capture_dir(path: &Path) -> bool {
    path.join(CAPTURE_FILE).is_file()
}

fn read_capture(dir: &Path) -> Result<(Capture, String), String> {
    let read = |file: &str| {
        let path = dir.join(file);
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
    };
    let meta: Capture = serde_json::from_str(&read(CAPTURE_FILE)?)
        .map_err(|e| format!("{}: {e}", dir.join(CAPTURE_FILE).display()))?;
    Ok((meta, read(SNAPSHOT_FILE)?))
}

/// `--replay <dir>`: reruns a captured generator on its snapshot and reports whether the result
/// still matches the captured one.
pub fn replay(config: &Arc<ToolchainConfig>, dir: &Path) -> Result<String, String> {
    let (meta, snapshot) = read_capture(dir)?;
    if !meta.bin.is_file() {
        return Err(format!(
            "the captured executable {} is gone (cache cleared?); capture again with `dream generate --capture {}`",
            meta.bin.display(),
            meta.generator
        ));
    }
    let exe = GenExe {
        ll: meta.ll.clone(),
        bin: meta.bin.clone(),
    };
    let result = super::run::run(config, &exe, &meta.generator, &snapshot, meta.timeout_secs)?;
    let captured: Option<GenResult> = std::fs::read_to_string(dir.join(RESULT_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let verdict = match captured {
        Some(c) if c == result => "matches the captured result",
        Some(_) => "differs from the captured result",
        None => "no captured result to compare against",
    };
    Ok(format!(
        "replayed '{}': {verdict}\n{}",
        meta.generator,
        summarize(&result)
    ))
}

/// `--verify-incremental`: reruns every `@incremental` generator that would replay a cached
/// result and fails when the fresh result differs (the generator reads something its snapshot
/// does not capture).
pub fn verify_incremental(
    config: &Arc<ToolchainConfig>,
    insp: &GenInspection,
) -> Result<String, String> {
    let mut out = String::new();
    let mut mismatched = Vec::new();
    let cache_root = config.generator_cache_root();
    for g in insp.generators.iter().filter(|g| g.registered.incremental) {
        let (Some(key), Some(snapshot)) = (&g.result_key, &g.snapshot_json) else {
            let _ = writeln!(out, "{}: skipped, no trigger matches", g.registered.name);
            continue;
        };
        let Some(cached) = super::incremental::load(&cache_root, key) else {
            let _ = writeln!(
                out,
                "{}: no cached result yet (build once first)",
                g.registered.name
            );
            continue;
        };
        let exe = executable(config, &g.plan)?;
        let fresh = super::run::run(config, &exe, &g.registered.name, snapshot, timeout_of(g))?;
        if fresh == cached {
            let _ = writeln!(
                out,
                "{}: ok, fresh run matches the cached result",
                g.registered.name
            );
        } else {
            let _ = writeln!(
                out,
                "{}: MISMATCH, fresh run differs from the cached result",
                g.registered.name
            );
            mismatched.push(g.registered.name.clone());
        }
    }
    if mismatched.is_empty() {
        Ok(out)
    } else {
        Err(format!(
            "{out}@incremental contract violated by {}: its output depends on input outside the snapshot",
            mismatched.join(", ")
        ))
    }
}

/// What `dream debug-adapter --generator` launches under lldb-dap.
pub struct DebugLaunch {
    pub generator: String,
    pub exe: GenExe,
    pub args: Vec<String>,
    pub result: PathBuf,
}

fn launch_files(
    config: &ToolchainConfig,
    name: &str,
    exe: GenExe,
    snapshot_text: &str,
) -> Result<DebugLaunch, String> {
    let dir = config.generator_cache_root().join("debug");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let snapshot_path = dir.join(format!("{name}.snapshot.json"));
    let result = dir.join(format!("{name}.result.json"));
    let _ = std::fs::remove_file(&result);
    std::fs::write(&snapshot_path, snapshot_text)
        .map_err(|e| format!("write {}: {e}", snapshot_path.display()))?;
    Ok(DebugLaunch {
        generator: name.to_string(),
        args: super::run::exe_args(name, &snapshot_path, &result),
        exe,
        result,
    })
}

/// A debug session for a capture directory: no front end, the captured executable and snapshot.
pub fn debug_launch_from_capture(
    config: &ToolchainConfig,
    dir: &Path,
    name: &str,
) -> Result<DebugLaunch, String> {
    let (meta, snapshot) = read_capture(dir)?;
    if meta.generator != name {
        return Err(format!(
            "{} captured generator '{}', not '{name}'",
            dir.display(),
            meta.generator
        ));
    }
    if !meta.bin.is_file() {
        return Err(format!(
            "the captured executable {} is gone (cache cleared?); capture again with `dream generate --capture {name}`",
            meta.bin.display()
        ));
    }
    let exe = GenExe {
        ll: meta.ll,
        bin: meta.bin,
    };
    launch_files(config, name, exe, &snapshot)
}

/// Prepares a debug session: the cached executable plus the snapshot (`snapshot`, or the one
/// the program produces now) written where the launched process reads it.
pub fn debug_launch(
    config: &Arc<ToolchainConfig>,
    insp: &GenInspection,
    name: &str,
    snapshot: Option<&Path>,
) -> Result<DebugLaunch, String> {
    let g = find(insp, name)?;
    let snapshot_text = match snapshot {
        Some(path) => {
            let path = if path.is_dir() {
                path.join(SNAPSHOT_FILE)
            } else {
                path.to_path_buf()
            };
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?
        }
        None => snapshot_of(g)?.to_string(),
    };
    let exe = executable(config, &g.plan)?;
    launch_files(config, name, exe, &snapshot_text)
}

/// The result a finished debug session left behind, summarized.
pub fn debug_summary(launch: &DebugLaunch) -> String {
    match std::fs::read_to_string(&launch.result)
        .ok()
        .and_then(|t| serde_json::from_str::<GenResult>(&t).ok())
    {
        Some(result) => format!(
            "generator '{}' finished:\n{}",
            launch.generator,
            summarize(&result)
        ),
        None => format!(
            "generator '{}' wrote no result (the session ended before `ctx.finish()`)",
            launch.generator
        ),
    }
}

/// The program `--prewarm` inspects: one that loads every std package owning a generator, so
/// each std generator registers and its executable plan is known.
pub fn prewarm_entry(config: &ToolchainConfig) -> std::io::Result<PathBuf> {
    let dir = config.generator_cache_root().join("prewarm");
    std::fs::create_dir_all(&dir)?;
    let mut src = String::new();
    for pkg in dream_stdlib::STD_PACKAGES {
        if !pkg.generators.is_empty() {
            let _ = writeln!(src, "import {};", pkg.name);
        }
    }
    src.push_str("\nfun main(): void {\n}\n");
    let entry = dir.join("main.dream");
    std::fs::write(&entry, src)?;
    Ok(entry)
}

/// `--prewarm`: builds every std generator executable, so the first build that uses one only
/// launches it. Returns one line per executable.
pub fn prewarm(config: &Arc<ToolchainConfig>, insp: &GenInspection) -> Result<String, String> {
    let mut out = String::new();
    let mut seen = std::collections::BTreeSet::new();
    for g in insp.generators.iter().filter(|g| g.registered.is_std()) {
        if !seen.insert(g.plan.key.clone()) {
            continue;
        }
        let (exe, built) = ensure_built(config, &g.plan)?;
        let _ = writeln!(
            out,
            "{} {} ({})",
            if built { "built" } else { "cached" },
            g.plan.generators.join(", "),
            exe.bin.display()
        );
    }
    Ok(out)
}
