use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cached {
    inputs: Vec<(PathBuf, Option<String>)>,
    result: serde_json::Value,
}

fn fingerprint(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
}

pub(super) fn run(root: &Path, request: &serde_json::Value) -> Result<serde_json::Value, Error> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(super::tool::identity().as_bytes());
    hasher.update(&serde_json::to_vec(request).map_err(Error::other)?);
    let directory = root.join(".dream/js");
    fs::create_dir_all(&directory)?;
    let lock = fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(".cache-lock"))?;
    lock.lock()?;
    let path = directory.join(format!("{}.json", hasher.finalize().to_hex()));
    if let Ok(bytes) = fs::read(&path)
        && let Ok(cached) = serde_json::from_slice::<Cached>(&bytes)
        && cached
            .inputs
            .iter()
            .all(|(path, hash)| fingerprint(path) == *hash)
    {
        return Ok(cached.result);
    }
    let result = super::tool::run(root, request)?;
    let mut paths = std::collections::BTreeSet::from([
        root.join("dream.toml"),
        root.join("dream.npm.lock"),
        root.join("dream.js.lock"),
        root.join(".dream/npm/.installed"),
    ]);
    if let Some(modules) = result["modules"].as_array() {
        for module in modules {
            for path in module["inputs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|path| path.as_str())
            {
                paths.insert(PathBuf::from(path));
            }
        }
    }
    let cached = Cached {
        inputs: paths
            .into_iter()
            .map(|path| {
                let hash = fingerprint(&path);
                (path, hash)
            })
            .collect(),
        result,
    };
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec(&cached).map_err(Error::other)?,
    )?;
    fs::rename(temporary, path)?;
    Ok(cached.result)
}
