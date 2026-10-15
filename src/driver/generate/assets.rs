use super::model::ToolFile;
use base64::Engine;

/// Tool files are untrusted even on replay; decoding and hashes follow the same path.
pub(super) fn file_bytes(file: &ToolFile) -> Result<Vec<u8>, String> {
    let bytes = if file.binary {
        base64::engine::general_purpose::STANDARD
            .decode(&file.contents)
            .map_err(|e| e.to_string())?
    } else {
        file.contents.as_bytes().to_vec()
    };
    if blake3::hash(&bytes).to_hex().as_str() != file.hash {
        return Err(format!("tool output '{}' has an invalid hash", file.path));
    }
    Ok(bytes)
}
