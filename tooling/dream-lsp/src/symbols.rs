//! Text scan of Dream sources for auto-import. The compiler does not use this.

use dream_stdlib::{BOOTSTRAP_PACKAGES, STD_PACKAGES};

/// Maps a public top-level stdlib symbol name to the package that exports it.
/// Bootstrap packages are skipped so auto-import does not suggest them.
pub fn symbol_to_package() -> std::collections::HashMap<String, &'static str> {
    let mut map = std::collections::HashMap::new();
    for pkg in STD_PACKAGES {
        if BOOTSTRAP_PACKAGES.contains(&pkg.name) {
            continue;
        }
        for &(_, src) in pkg.files {
            for name in public_top_level_names(src) {
                map.entry(name).or_insert(pkg.name);
            }
            for name in top_level_extend_targets(src) {
                map.entry(name).or_insert(pkg.name);
            }
        }
    }
    map
}

fn top_level_extend_targets(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in src.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("module ") || t.starts_with("import ") {
            continue;
        }
        let rest = t.strip_prefix("public ").unwrap_or(t).trim_start();
        let Some(after) = rest.strip_prefix("extend ") else {
            continue;
        };
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

/// Public top-level declaration names in a Dream source string.
/// Only column-0 `public class|enum|interface|fun|extend|struct|union` decls.
pub fn public_top_level_names(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("module ") || t.starts_with("import ") {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let rest = if let Some(r) = t.strip_prefix("public ") {
            r.trim_start()
        } else {
            continue;
        };
        let rest = rest
            .strip_prefix("sealed ")
            .unwrap_or(rest)
            .strip_prefix("static ")
            .unwrap_or(rest)
            .strip_prefix("async ")
            .unwrap_or(rest)
            .trim_start();
        for kind in [
            "class ",
            "enum ",
            "interface ",
            "fun ",
            "extend ",
            "struct ",
            "union ",
        ] {
            if let Some(after) = rest.strip_prefix(kind) {
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    names.push(name);
                }
                break;
            }
        }
    }
    names
}
