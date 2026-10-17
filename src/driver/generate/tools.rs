use super::model::{Output, ToolFile, ToolInfo, ToolResult};
use crate::driver::toolchain::ToolchainConfig;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Limit for a `dream.toml` pin that sets no `timeout_secs`.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: String,
    version: String,
    /// BLAKE3 of the executable named by `path`.
    hash: String,
    /// Install tree the tool may read (sibling libraries, data files); fingerprinted whole.
    root: Option<String>,
    timeout_secs: Option<u64>,
}

pub(super) struct ResolvedTool {
    binary: PathBuf,
    root: Option<PathBuf>,
    timeout: Duration,
    info: ToolInfo,
}

fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn hash_tree_into(hasher: &mut blake3::Hasher, root: &Path, dir: &Path) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        hasher.update(&(name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        if kind.is_symlink() {
            let target =
                std::fs::read_link(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            hasher.update(b"l");
            hasher.update(target.to_string_lossy().as_bytes());
        } else if kind.is_dir() {
            hasher.update(b"d");
            hash_tree_into(hasher, root, &path)?;
        } else {
            hasher.update(b"f");
            let mut file =
                std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut contents = blake3::Hasher::new();
            std::io::copy(&mut file, &mut contents)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            hasher.update(contents.finalize().as_bytes());
        }
    }
    Ok(())
}

/// One BLAKE3 over every path and file of an install tree in sorted order, so a changed sibling
/// library invalidates cached results. Trees are hashed once per process.
fn tree_fingerprint(root: &Path) -> Result<String, String> {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(root) {
        return Ok(hit.clone());
    }
    let mut hasher = blake3::Hasher::new();
    hash_tree_into(&mut hasher, root, root)?;
    let fingerprint = hasher.finalize().to_hex().to_string();
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(root.to_path_buf(), fingerprint.clone());
    Ok(fingerprint)
}

fn canonical(name: &str, path: &Path) -> Result<PathBuf, String> {
    dunce::canonicalize(path).map_err(|e| format!("tool '{name}': {}: {e}", path.display()))
}

fn from_registry(
    config: &ToolchainConfig,
    name: &str,
    tool: &'static dream_abi::toolchain::PinnedTool,
    relative: &str,
) -> Result<ResolvedTool, String> {
    let located = config
        .locate_tool(tool)
        .map_err(|e| format!("tool '{name}': {e}"))?;
    let binary = canonical(name, &located.executable(relative))?;
    let root = located
        .owns_root
        .then(|| canonical(name, &located.root))
        .transpose()?;
    let fingerprint = match &root {
        Some(root) => tree_fingerprint(root),
        None => hash_file(&binary),
    }
    .map_err(|e| format!("tool '{name}': {e}"))?;
    Ok(ResolvedTool {
        binary,
        root,
        timeout: Duration::from_secs(tool.timeout_secs),
        info: ToolInfo {
            name: name.to_string(),
            version: tool.version.to_string(),
            fingerprint,
            available: true,
        },
    })
}

fn from_pin(project: &Path, name: &str, value: toml::Value) -> Result<ResolvedTool, String> {
    let pin: Pin = value
        .try_into()
        .map_err(|e| format!("tool '{name}': {e}"))?;
    if pin.version.is_empty() || pin.hash.len() != 64 || pin.timeout_secs == Some(0) {
        return Err(format!(
            "tool '{name}' needs a version, a BLAKE3 hash and a positive timeout_secs"
        ));
    }
    let binary = canonical(name, &project.join(&pin.path))?;
    let actual = hash_file(&binary)?;
    if actual != pin.hash {
        return Err(format!(
            "tool '{name}' does not match its pinned BLAKE3 hash (the executable hashes to {actual})"
        ));
    }
    let root = pin
        .root
        .map(|root| canonical(name, &project.join(root)))
        .transpose()?;
    if root.as_ref().is_some_and(|root| !binary.starts_with(root)) {
        return Err(format!(
            "tool '{name}': `path` must be inside the pinned `root`"
        ));
    }
    let fingerprint = match &root {
        Some(root) => tree_fingerprint(root).map_err(|e| format!("tool '{name}': {e}"))?,
        None => pin.hash,
    };
    Ok(ResolvedTool {
        binary,
        root,
        timeout: Duration::from_secs(pin.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS)),
        info: ToolInfo {
            name: name.to_string(),
            version: pin.version,
            fingerprint,
            available: true,
        },
    })
}

/// The shared pinned-tool table first, then the project's `dream.toml` `[tools]` pins.
fn resolve(config: &ToolchainConfig, project: &Path, name: &str) -> Result<ResolvedTool, String> {
    if let Some((tool, relative)) = dream_abi::toolchain::tool_providing(name) {
        return from_registry(config, name, tool, relative);
    }
    let manifest = project.join("dream.toml");
    let pin = if manifest.is_file() {
        let text = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
        let parsed: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
        parsed
            .get("tools")
            .and_then(|tools| tools.get(name))
            .cloned()
    } else {
        None
    };
    match pin {
        Some(value) => from_pin(project, name, value),
        None => Err(format!(
            "tool '{name}' is not in the Dream pinned-tool registry or pinned in dream.toml [tools]"
        )),
    }
}

/// Fingerprints declared tools before snapshot serialization so every cache key includes them.
/// A tool that cannot be resolved is described as unavailable, so a generator can skip an
/// optional step; requesting it fails with the resolution error.
pub(super) fn describe(
    config: &Arc<ToolchainConfig>,
    root: &Path,
    names: &[String],
) -> Vec<ToolInfo> {
    names
        .iter()
        .map(|name| match resolve(config, root, name) {
            Ok(tool) => tool.info,
            Err(_) => ToolInfo {
                name: name.clone(),
                ..ToolInfo::default()
            },
        })
        .collect()
}

/// One finished tool request: the transcript form of its result (file bytes stripped) and the
/// bytes of each file in the same order.
pub(super) struct ToolRun {
    pub result: ToolResult,
    pub bytes: Vec<Vec<u8>>,
}

/// Executes with an isolated filesystem and no network; unsupported hosts fail closed.
pub(super) fn run(
    config: &Arc<ToolchainConfig>,
    root: &Path,
    request: &Output,
    expected: &[ToolInfo],
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<ToolRun, String> {
    let Output::ToolRun {
        id,
        tool,
        args,
        inputs,
        outputs,
        opaque_outputs,
    } = request
    else {
        return Err("expected tool request".into());
    };
    let resolved = resolve(config, root, tool)?;
    if !expected.contains(&resolved.info) {
        return Err(format!(
            "tool '{tool}' changed after the snapshot was built"
        ));
    }
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dir = dunce::canonicalize(scratch.path()).map_err(|e| e.to_string())?;
    for input in inputs {
        let contents = artifacts
            .get(input)
            .ok_or_else(|| format!("tool '{id}' input '{input}' is not an emitted artifact"))?;
        let path = dir.join(input);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, contents).map_err(|e| e.to_string())?;
    }
    for output in outputs.iter().chain(opaque_outputs) {
        if let Some(parent) = dir.join(output).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let output = super::tool_sandbox::run(&super::tool_sandbox::Invocation {
        binary: &resolved.binary,
        args,
        scratch: &dir,
        resources: resolved.root.as_deref(),
        timeout: resolved.timeout,
    })?;
    let mut files = Vec::new();
    let mut bytes = Vec::new();
    for (path, opaque) in outputs
        .iter()
        .map(|p| (p, false))
        .chain(opaque_outputs.iter().map(|p| (p, true)))
    {
        if !dir.join(path).exists() {
            continue;
        }
        let mut current = dir.clone();
        for part in path.split('/') {
            current.push(part);
            if std::fs::symlink_metadata(&current)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err(format!("tool '{id}' output '{path}' contains a symlink"));
            }
        }
        let contents =
            std::fs::read(&current).map_err(|e| format!("tool '{id}' output '{path}': {e}"))?;
        files.push(ToolFile {
            path: path.clone(),
            contents: String::new(),
            binary: std::str::from_utf8(&contents).is_err(),
            hash: blake3::hash(&contents).to_hex().to_string(),
            size: contents.len() as u64,
            opaque,
        });
        bytes.push(contents);
    }
    Ok(ToolRun {
        result: ToolResult {
            id: id.clone(),
            exit: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: if !output.status.success() && output.stderr.is_empty() {
                format!("tool terminated with {}", output.status)
            } else {
                String::from_utf8_lossy(&output.stderr).into_owned()
            },
            files,
        },
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_fingerprints_cover_sibling_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("tool");
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("tool"), "exe").unwrap();
        std::fs::write(root.join("lib/support"), "v1").unwrap();
        let mut first = blake3::Hasher::new();
        hash_tree_into(&mut first, &root, &root).unwrap();
        std::fs::write(root.join("lib/support"), "v2").unwrap();
        let mut second = blake3::Hasher::new();
        hash_tree_into(&mut second, &root, &root).unwrap();
        assert_ne!(first.finalize(), second.finalize());
    }

    #[test]
    fn unresolvable_tools_are_described_as_unavailable() {
        let project = tempfile::tempdir().unwrap();
        let config = Arc::new(ToolchainConfig::default());
        let tools = describe(&config, project.path(), &["no-such-tool".to_string()]);
        assert_eq!(
            tools,
            vec![ToolInfo {
                name: "no-such-tool".into(),
                ..ToolInfo::default()
            }]
        );
        assert!(!tools[0].available);
    }

    #[test]
    fn requests_need_distinct_contained_outputs() {
        let request = |outputs: &[&str], opaque: &[&str]| Output::ToolRun {
            id: "compile".into(),
            tool: "copy".into(),
            args: Vec::new(),
            inputs: vec!["in.txt".into()],
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
            opaque_outputs: opaque.iter().map(|s| s.to_string()).collect(),
        };
        let declared = vec!["copy".to_string()];
        assert!(super::super::artifacts::valid_tool_request(
            &request(&["a.txt"], &["b.bin"]),
            &declared
        ));
        assert!(!super::super::artifacts::valid_tool_request(
            &request(&["a.txt"], &["a.txt"]),
            &declared
        ));
        assert!(!super::super::artifacts::valid_tool_request(
            &request(&["../a.txt"], &[]),
            &declared
        ));
        assert!(!super::super::artifacts::valid_tool_request(
            &request(&[], &["outputs.json"]),
            &declared
        ));
    }
}
