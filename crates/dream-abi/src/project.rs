//! Project layout shared by the compiler driver and `dreamer`.

use std::path::{Path, PathBuf};

pub const MANIFEST_FILE_NAME: &str = "dream.toml";

/// The nearest directory at or above `start` (a file or directory) that holds `dream.toml`.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if dir.join(MANIFEST_FILE_NAME).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}
