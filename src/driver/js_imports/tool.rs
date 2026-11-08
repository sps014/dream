use std::fs::{self, File};
use std::io::{Error, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SOURCES: &[(&str, &str)] = &[
    (
        "keywords.json",
        include_str!("../../../tooling/js-bindings/keywords.json"),
    ),
    (
        "package.json",
        include_str!("../../../tooling/js-bindings/package.json"),
    ),
    (
        "package-lock.json",
        include_str!("../../../tooling/js-bindings/package-lock.json"),
    ),
    (
        "main.mjs",
        include_str!("../../../tooling/js-bindings/main.mjs"),
    ),
    (
        "dependencies.mjs",
        include_str!("../../../tooling/js-bindings/dependencies.mjs"),
    ),
    (
        "remote.mjs",
        include_str!("../../../tooling/js-bindings/remote.mjs"),
    ),
    (
        "resolve.mjs",
        include_str!("../../../tooling/js-bindings/resolve.mjs"),
    ),
    (
        "inspect.mjs",
        include_str!("../../../tooling/js-bindings/inspect.mjs"),
    ),
    (
        "types.mjs",
        include_str!("../../../tooling/js-bindings/types.mjs"),
    ),
    (
        "emit.mjs",
        include_str!("../../../tooling/js-bindings/emit.mjs"),
    ),
];

pub(super) fn identity() -> String {
    let mut hash = blake3::Hasher::new();
    for (name, contents) in SOURCES {
        hash.update(name.as_bytes());
        hash.update(contents.as_bytes());
    }
    hash.finalize().to_hex().to_string()
}

fn directory() -> Result<PathBuf, Error> {
    let home = std::env::var_os("DREAM_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".dream"))
        })
        .ok_or_else(|| Error::other("JS bindings need DREAM_HOME or HOME for the tool cache"))?;
    let directory = home.join("tools/js-bindings").join(identity());
    fs::create_dir_all(&directory)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(".lock"))?;
    lock.lock()?;
    for (name, contents) in SOURCES {
        let path = directory.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::read_to_string(&path).ok().as_deref() != Some(contents) {
            fs::write(path, contents)?;
        }
    }
    if !directory
        .join("node_modules/oxc-parser/package.json")
        .is_file()
    {
        if std::env::var("DREAM_NO_AUTO_INSTALL").as_deref() == Ok("1") {
            return Err(Error::other(
                "JS binding tools are not cached and DREAM_NO_AUTO_INSTALL=1 disables installation",
            ));
        }
        let output = Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" })
            .args([
                "ci",
                "--omit=dev",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ])
            .current_dir(&directory)
            .output()
            .map_err(|e| {
                Error::other(format!("automatic JS bindings require Node and npm: {e}"))
            })?;
        if !output.status.success() {
            return Err(Error::other(format!(
                "installing JS binding tools: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
    }
    Ok(directory)
}

pub(super) fn run(root: &Path, request: &serde_json::Value) -> Result<serde_json::Value, Error> {
    let directory = directory()?;
    let scratch = root.join(".dream/js");
    fs::create_dir_all(&scratch)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(scratch.join(".lock"))?;
    lock.lock()?;
    invoke(&directory, root, request)
}

fn invoke(
    directory: &Path,
    cwd: &Path,
    request: &serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let mut child = Command::new("node")
        .arg(directory.join("main.mjs"))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::other(format!("automatic JS bindings require Node: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&serde_json::to_vec(request).map_err(Error::other)?)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(Error::other)
}
