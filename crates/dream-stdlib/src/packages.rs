use crate::{BOOTSTRAP_PACKAGES, STD_PACKAGES, StdPackage};
use indexmap::IndexSet;

/// Returns every `(virtual_path, source)` across all packages in deterministic registry order.
pub fn all_prelude_files() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for pkg in STD_PACKAGES {
        out.extend_from_slice(pkg.files);
    }
    out
}

/// Looks up a package by dotted name (`system.io`).
pub fn find_package(name: &str) -> Option<&'static StdPackage> {
    STD_PACKAGES.iter().find(|p| p.name == name)
}

/// Resolve a declaration's exact embedded source, not a user-controlled path prefix.
pub fn package_for_source(path: &str) -> Option<&'static StdPackage> {
    STD_PACKAGES
        .iter()
        .find(|package| package.files.iter().any(|(source, _)| *source == path))
}

/// True when `slash_path` (parser form of a plain import, e.g. `system/io`) names a std package.
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

/// `(field, capability)` for every `@runtime("field", "capability", …)` in the embedded sources
/// whose host is a capability library rather than core, sorted by field. Host contract tests compare it with each capability library's exports.
pub fn runtime_host_fields() -> Vec<(&'static str, &'static str)> {
    let marker = "@runtime(\"";
    let mut out = Vec::new();
    for package in STD_PACKAGES {
        for &(_, source) in package.files {
            let mut rest = source;
            while let Some(at) = rest.find(marker) {
                rest = &rest[at + marker.len()..];
                let Some(close) = rest.find(')') else {
                    break;
                };
                let args: Vec<&str> = rest[..close]
                    .split(',')
                    .map(|a| a.trim().trim_matches('"'))
                    .collect();
                if let [field, capability, ..] = args[..]
                    && capability != "core"
                {
                    out.push((field, capability));
                }
                rest = &rest[close..];
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `(export_name, runtime symbol)` pairs the package owning `source` exports when its host import
/// `field` is used.
pub fn guest_exports_for(source: &str, field: &str) -> Vec<(String, String)> {
    package_for_source(source)
        .into_iter()
        .flat_map(|package| package.guest_exports)
        .filter(|export| export.import == field)
        .map(|export| (export.export_name.to_string(), export.symbol.to_string()))
        .collect()
}

pub fn package_declares_runtime_field(package: &StdPackage, field: &str) -> bool {
    let marker = format!("@runtime(\"{field}\"");
    package
        .files
        .iter()
        .any(|(_, source)| source.contains(&marker))
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
