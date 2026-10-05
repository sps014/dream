use crate::{BOOTSTRAP_PACKAGES, STD_PACKAGES};

/// Maps a public top-level stdlib symbol name to the package that exports it (for LSP auto-import).
/// Built by scanning package sources for `public class|enum|interface|fun|extend` at file top-level.
pub fn symbol_to_package() -> std::collections::HashMap<String, &'static str> {
    let mut map = std::collections::HashMap::new();
    for pkg in STD_PACKAGES {
        // Bootstrap packages need no user import — skip so auto-import won't suggest them.
        if BOOTSTRAP_PACKAGES.contains(&pkg.name) {
            continue;
        }
        for &(_, src) in pkg.files {
            for name in public_top_level_names(src) {
                map.entry(name).or_insert(pkg.name);
            }
            // `extend System` in `system` (not `public`) still means `System.print` needs that
            // import; bootstrap defines the type, the package adds the methods.
            for name in top_level_extend_targets(src) {
                map.entry(name).or_insert(pkg.name);
            }
        }
    }
    map
}

/// Column-0 `extend Type` targets (with or without `public`), for auto-import of package
/// extensions on bootstrap types (`System`, …).
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

/// Public top-level declaration names in a Dream source string (for LSP auto-import).
/// Only column-0 `public class|enum|interface|fun|extend|struct|union` decls; nested/indented
/// members are ignored.
pub fn public_top_level_names(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("module ") || t.starts_with("import ") {
            continue;
        }
        // Nested members are indented; only look at column-0 public decls (approx).
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
