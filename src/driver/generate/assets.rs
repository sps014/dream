use super::model::GenResult;
#[cfg(feature = "native")]
use super::model::ToolFile;
use base64::Engine;
use std::collections::BTreeMap;

/// Prefix of an asset or publish source naming a tool output: `tool:<request id>/<path>`.
pub(super) const TOOL_SOURCE: &str = "tool:";

#[cfg(any(test, feature = "native"))]
fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Transcript blobs are untrusted even on replay; every read re-checks the content address.
pub(super) fn blob_bytes(blobs: &BTreeMap<String, String>, hash: &str) -> Result<Vec<u8>, String> {
    let encoded = blobs
        .get(hash)
        .ok_or_else(|| format!("tool output blob {hash} is missing"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    if blake3::hash(&bytes).to_hex().as_str() != hash {
        return Err(format!("tool output blob {hash} has an invalid hash"));
    }
    Ok(bytes)
}

#[cfg(any(test, feature = "native"))]
pub(super) fn store_blob(blobs: &mut BTreeMap<String, String>, hash: &str, bytes: &[u8]) {
    blobs
        .entry(hash.to_string())
        .or_insert_with(|| encode(bytes));
}

/// The snapshot form of a tool file: readable outputs carry their bytes inline (text, or base64
/// when binary); opaque outputs only their path, hash and size.
#[cfg(feature = "native")]
pub(super) fn snapshot_file(file: &ToolFile, bytes: &[u8]) -> ToolFile {
    let mut file = file.clone();
    file.contents = match (file.opaque, std::str::from_utf8(bytes)) {
        (true, _) => String::new(),
        (false, Ok(text)) => text.to_string(),
        (false, Err(_)) => encode(bytes),
    };
    file
}

/// Resolves `tool:<id>/<path>` against the recorded feedback of every phase.
pub(super) fn tool_output(result: &GenResult, source: &str) -> Option<Result<Vec<u8>, String>> {
    let (id, path) = source.strip_prefix(TOOL_SOURCE)?.split_once('/')?;
    let file = result
        .phases
        .iter()
        .flat_map(|p| &p.tool_results)
        .filter(|r| r.id == id)
        .flat_map(|r| &r.files)
        .find(|f| f.path == path)?;
    Some(blob_bytes(&result.blobs, &file.hash))
}
