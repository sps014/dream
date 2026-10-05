use crate::{StdPackage, BOOTSTRAP_PACKAGES, STD_PACKAGES};
use indexmap::IndexSet;

/// Returns every `(virtual_path, source)` across all packages in deterministic registry order.
pub fn all_prelude_files() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for pkg in STD_PACKAGES {
        out.extend_from_slice(pkg.files);
    }
    out
}

/// Looks up a package by dotted name (`system.net`).
pub fn find_package(name: &str) -> Option<&'static StdPackage> {
    STD_PACKAGES.iter().find(|p| p.name == name)
}

/// Resolve a declaration's exact embedded source, not a user-controlled path prefix.
pub fn package_for_source(path: &str) -> Option<&'static StdPackage> {
    STD_PACKAGES
        .iter()
        .find(|package| package.files.iter().any(|(source, _)| *source == path))
}

/// True when `slash_path` (parser form of a plain import, e.g. `system/net`) names a std package.
pub fn std_package_from_slash_path(slash_path: &str) -> Option<&'static StdPackage> {
    let dotted = slash_path.replace('/', ".");
    find_package(&dotted)
}

/// Expands `requested` package names with bootstrap + transitive deps, in registry merge order.
pub fn resolve_packages_to_load(requested: &IndexSet<String>) -> Vec<&'static StdPackage> {
    let mut needed: IndexSet<&'static str> = IndexSet::new();
    for &boot in BOOTSTRAP_PACKAGES {
        needed.insert(boot);
    }
    for name in requested {
        collect_deps(name, &mut needed);
    }
    STD_PACKAGES
        .iter()
        .filter(|p| needed.contains(p.name))
        .collect()
}

fn collect_deps(name: &str, needed: &mut IndexSet<&'static str>) {
    let Some(pkg) = find_package(name) else {
        return;
    };
    if !needed.insert(pkg.name) {
        return;
    }
    for &dep in pkg.deps {
        collect_deps(dep, needed);
    }
}
