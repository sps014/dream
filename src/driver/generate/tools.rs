use super::artifacts::valid_relative_path;
use super::model::{Output, ToolFile, ToolInfo, ToolResult};
use crate::driver::toolchain::ToolchainConfig;
use base64::Engine;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: String,
    version: String,
    hash: String,
}

fn resolve(
    config: &Arc<ToolchainConfig>,
    root: &Path,
    name: &str,
) -> Result<(PathBuf, ToolInfo), String> {
    let manifest = root.join("dream.toml");
    let pins = if manifest.is_file() {
        let text = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
        let parsed: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
        parsed
            .get("tools")
            .and_then(|tools| tools.get(name))
            .cloned()
    } else {
        None
    };
    let (path, version, expected) = if let Some(value) = pins {
        let pin: Pin = value
            .try_into()
            .map_err(|e| format!("tool '{name}': {e}"))?;
        if pin.version.is_empty() || pin.hash.len() != 64 {
            return Err(format!("tool '{name}' needs a version and BLAKE3 hash"));
        }
        (root.join(pin.path), pin.version, Some(pin.hash))
    } else if name == "wasm-opt" {
        (
            crate::execution::llvm::wasm_opt::resolve(config)?,
            dream_abi::toolchain::BINARYEN_VERSION.to_string(),
            None,
        )
    } else if matches!(
        name,
        "clang" | "clang++" | "opt" | "llc" | "llvm-as" | "llvm-dis" | "llvm-link"
    ) {
        let tools = crate::execution::llvm::tools::resolve_llvm(config)?;
        (
            tools.tool(name),
            crate::execution::llvm::tools::LLVM_VERSION.to_string(),
            None,
        )
    } else {
        return Err(format!(
            "tool '{name}' is not pinned in dream.toml or the Dream toolchain"
        ));
    };
    let path = dunce::canonicalize(path).map_err(|e| format!("tool '{name}': {e}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("tool '{name}': {e}"))?;
    let fingerprint = blake3::hash(&bytes).to_hex().to_string();
    if expected.is_some_and(|hash| hash != fingerprint) {
        return Err(format!(
            "tool '{name}' does not match its pinned BLAKE3 hash"
        ));
    }
    Ok((
        path,
        ToolInfo {
            name: name.to_string(),
            version,
            fingerprint,
        },
    ))
}

/// Fingerprints declared tools before snapshot serialization so every cache key includes them.
pub(super) fn describe(
    config: &Arc<ToolchainConfig>,
    root: &Path,
    names: &[String],
) -> Result<Vec<ToolInfo>, String> {
    names
        .iter()
        .map(|name| resolve(config, root, name).map(|(_, info)| info))
        .collect()
}

/// Rejects undeclared tools and paths that could escape the tool scratch directory.
pub(super) fn valid_request(output: &Output, declared: &[String]) -> bool {
    match output {
        Output::ToolRun {
            id,
            tool,
            inputs,
            outputs,
            ..
        } => {
            !id.is_empty()
                && declared.contains(tool)
                && inputs
                    .iter()
                    .chain(outputs)
                    .all(|p| valid_relative_path(p) && p != "outputs.json")
        }
        _ => false,
    }
}

/// Executes with an isolated filesystem and no network; unsupported hosts fail closed.
pub(super) fn run(
    config: &Arc<ToolchainConfig>,
    root: &Path,
    request: &Output,
    expected: &[ToolInfo],
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<ToolResult, String> {
    let Output::ToolRun {
        id,
        tool,
        args,
        inputs,
        outputs,
    } = request
    else {
        return Err("expected tool request".into());
    };
    let (binary, info) = resolve(config, root, tool)?;
    if !expected.contains(&info) {
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
    for output in outputs {
        if let Some(parent) = dir.join(output).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let resources = config
        .toolchains
        .iter()
        .chain(config.llvm.iter())
        .find(|root| binary.starts_with(root));
    let mut command = super::tool_sandbox::command(&binary, &dir, resources.map(|p| p.as_path()))?;
    command
        .args(args)
        .current_dir(&dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &dir)
        .env("TMPDIR", &dir)
        .env("LANG", "C")
        .env("LC_ALL", "C");
    let output = super::tool_sandbox::execute(command)?;
    let mut files = Vec::new();
    for path in outputs {
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
        let hash = blake3::hash(&contents).to_hex().to_string();
        let (contents, binary) = match String::from_utf8(contents) {
            Ok(text) => (text, false),
            Err(bytes) => (
                base64::engine::general_purpose::STANDARD.encode(bytes.as_bytes()),
                true,
            ),
        };
        files.push(ToolFile {
            path: path.clone(),
            contents,
            binary,
            hash,
        });
    }
    Ok(ToolResult {
        id: id.clone(),
        exit: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: if !output.status.success() && output.stderr.is_empty() {
            format!("tool terminated with {}", output.status)
        } else {
            String::from_utf8_lossy(&output.stderr).into_owned()
        },
        files,
    })
}
