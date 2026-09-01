//! Optional `[[generators]]` entries from a nearby `dream.toml`.

use std::path::{Path, PathBuf};

/// Walks from `entry_file`'s directory upward looking for `dream.toml`.
pub fn find_project_root(entry_file: &str) -> Option<PathBuf> {
    let start = Path::new(entry_file)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("dream.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Cache directory for a generator harness: `target/generators/<kind>-<fingerprint>` when a
/// project root is known, otherwise the system temp dir.
#[cfg(feature = "native")]
pub fn harness_cache_dir(entry_file: Option<&str>, kind: &str, fingerprint: u64) -> PathBuf {
    let name = format!("dream-{kind}-{fingerprint:x}");
    if let Some(entry) = entry_file {
        if let Some(root) = find_project_root(entry) {
            return root.join("target").join("generators").join(name);
        }
    }
    std::env::temp_dir().join(name)
}

/// Walks from `entry_file`'s directory upward looking for `dream.toml`; returns generator paths
/// resolved relative to the manifest directory.
pub fn load_manifest_generators(entry_file: &str) -> Vec<String> {
    let Some(dir) = find_project_root(entry_file) else {
        return Vec::new();
    };
    let candidate = dir.join("dream.toml");
    parse_generators_from_manifest(&candidate, &dir)
}

/// Walks from `start` (a file or directory) upward looking for `dream.toml`.
pub fn find_project_root_from(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if dir.join("dream.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// `[package].entry` from `dream.toml`, resolved against the manifest directory.
pub fn package_entry_path(project_root: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(project_root.join("dream.toml")).ok()?;
    let rel = parse_package_entry(&text)?;
    if rel.trim().is_empty() {
        return None;
    }
    Some(project_root.join(rel))
}

/// Default compile root when the CLI is given no file: nearest `dream.toml` + `[package].entry`.
pub fn default_compile_entry(cwd: &Path) -> Option<PathBuf> {
    let root = find_project_root_from(cwd)?;
    package_entry_path(&root)
}

fn parse_package_entry(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("entry") else {
            continue;
        };
        let rest = rest.trim();
        if !rest.starts_with('=') {
            continue;
        }
        let path = rest[1..].trim().trim_matches('"').trim_matches('\'');
        if path.is_empty() {
            return None;
        }
        return Some(path.to_string());
    }
    None
}

/// Minimal extraction of `[[generators]]` `path = "..."` entries (no full TOML dependency).
fn parse_generators_from_manifest(manifest: &Path, base: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_generators = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_generators = trimmed == "[[generators]]";
            continue;
        }
        if !in_generators {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("path") {
            let rest = rest.trim().trim_start_matches('=').trim();
            let path = rest.trim_matches('"').trim_matches('\'');
            if path.is_empty() {
                continue;
            }
            let resolved: PathBuf = base.join(path);
            if let Ok(canon) = resolved.canonicalize() {
                if let Some(s) = canon.to_str() {
                    out.push(s.to_string());
                }
            } else if let Some(s) = resolved.to_str() {
                out.push(s.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_package_entry_reads_quoted_path() {
        let toml = "[package]\nname = \"x\"\nentry = \"src/main.dream\"\n";
        assert_eq!(parse_package_entry(toml).as_deref(), Some("src/main.dream"));
    }

    #[test]
    fn parse_package_entry_ignores_other_tables() {
        let toml = "[dependencies]\nentry = \"nope.dream\"\n[package]\nentry = \"src/app.dream\"\n";
        assert_eq!(parse_package_entry(toml).as_deref(), Some("src/app.dream"));
    }

    #[test]
    fn default_compile_entry_joins_manifest_dir() {
        let root = std::env::temp_dir().join(format!(
            "dream-entry-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("dream.toml"),
            "[package]\nname = \"t\"\nentry = \"src/main.dream\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.dream"), "fun main() {}\n").unwrap();
        let nested = root.join("src");
        let entry = default_compile_entry(&nested).unwrap();
        assert_eq!(entry, root.join("src/main.dream"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
