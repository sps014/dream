//! `dream.toml` project manifest: package metadata, dependencies, dev-dependencies, scripts, and
//! registry aliases. Parsed with `serde` + `toml`, mirroring how Cargo reads `Cargo.toml`.

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE_NAME: &str = "dream.toml";

/// Whether this package is a runnable application (`bin`) or a library (`lib`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PackageType {
    #[default]
    Bin,
    Lib,
}

impl PackageType {
    pub fn as_str(self) -> &'static str {
        match self {
            PackageType::Bin => "bin",
            PackageType::Lib => "lib",
        }
    }
}

impl std::fmt::Display for PackageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Hosts a Dream project may declare in `[package].targets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunTarget {
    Native,
    Web,
    Node,
}

impl RunTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            RunTarget::Native => "native",
            RunTarget::Web => "web",
            RunTarget::Node => "node",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "native" => Ok(RunTarget::Native),
            "web" => Ok(RunTarget::Web),
            "node" => Ok(RunTarget::Node),
            other => bail!(
                "unknown target '{}': expected one of native, web, node",
                other
            ),
        }
    }
}

impl std::fmt::Display for RunTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parse a comma-separated target list (`"native,web"`), validating and deduplicating while
/// preserving first-seen order.
pub fn parse_target_list(spec: &str) -> Result<Vec<RunTarget>> {
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let t = RunTarget::parse(part)?;
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.is_empty() {
        bail!("target list must include at least one of native, web, node");
    }
    Ok(out)
}

/// Resolve which host `dreamer run` should execute.
///
/// - Empty/omitted `targets` → `native` (or the explicit `--target` escape hatch).
/// - Exactly one listed target → that host (explicit must match if provided).
/// - Multiple listed targets → `--target` is required and must be one of them.
pub fn resolve_run_target(targets: &[String], explicit: Option<&str>) -> Result<RunTarget> {
    let listed: Vec<RunTarget> = targets
        .iter()
        .map(|s| RunTarget::parse(s))
        .collect::<Result<Vec<_>>>()?;

    if let Some(sel) = explicit {
        let chosen = RunTarget::parse(sel)?;
        if listed.is_empty() {
            return Ok(chosen);
        }
        if listed.len() == 1 {
            if listed[0] != chosen {
                bail!(
                    "this project targets only '{}'; cannot run with --target {}",
                    listed[0],
                    chosen
                );
            }
            return Ok(chosen);
        }
        if !listed.contains(&chosen) {
            bail!(
                "--target {} is not in package.targets ({})",
                chosen,
                listed
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        return Ok(chosen);
    }

    match listed.len() {
        0 => Ok(RunTarget::Native),
        1 => Ok(listed[0]),
        _ => bail!(
            "package.targets lists multiple hosts ({}); pass --target <{}>",
            listed
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            listed
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join("|")
        ),
    }
}

/// `[workspace]` table for a multi-package repo root (explicit member paths only).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceMeta {
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Present on package manifests; absent on a virtual workspace root (`[workspace]` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageMeta>,
    /// Present on a workspace root (virtual or root-package + members).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceMeta>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: BTreeMap<String, Dependency>,
    /// Named shell commands a developer can invoke via their own shell (e.g. `sh -c
    /// "$(dreamer script start)"`). Not consumed directly by any `dreamer` subcommand in this
    /// version — kept as project metadata for tooling/documentation purposes.
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    /// Named registry aliases (`name -> base URL`), referenced from `[dependencies]` entries via
    /// `registry = "name"`. The `default` alias is used when a dependency omits `registry`.
    #[serde(default)]
    pub registries: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    /// `bin` (default) = runnable app; `lib` = library (no `entry`, not runnable).
    #[serde(default, rename = "type")]
    pub package_type: PackageType,
    #[serde(default)]
    pub edition: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Compiler entry point for `bin` packages, relative to the manifest directory
    /// (e.g. `src/main.dream`). Forbidden on `lib` packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Search keywords published into the registry catalog (`dreamer search` / catalog.json).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Optional host list (`native`, `web`, `node`). Empty means no preference — `dreamer run`
    /// defaults to native wasmtime execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    /// Optional app icon path relative to the manifest directory (PNG).
    /// - `dream run`: loaded from disk next to `dream.toml`.
    /// - `dreamer pack`: PNG bytes are copied into the single-file exe (no sidecar assets folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// A dependency requirement: either a bare semver requirement string (`"^1.2"`) or a detailed
/// table (`{ version = "...", path = "...", git = "..." }`), matching Cargo's `Cargo.toml` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Version(String),
    Detailed(DetailedDependency),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DetailedDependency {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub registry: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
}

impl Dependency {
    pub fn version_req(&self) -> Option<&str> {
        match self {
            Dependency::Version(v) => Some(v.as_str()),
            Dependency::Detailed(d) => d.version.as_deref(),
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            Dependency::Version(_) => None,
            Dependency::Detailed(d) => d.path.as_deref(),
        }
    }

    pub fn git(&self) -> Option<&str> {
        match self {
            Dependency::Version(_) => None,
            Dependency::Detailed(d) => d.git.as_deref(),
        }
    }

    pub fn registry_alias(&self) -> Option<&str> {
        match self {
            Dependency::Version(_) => None,
            Dependency::Detailed(d) => d.registry.as_deref(),
        }
    }

    pub fn detailed(&self) -> DetailedDependency {
        match self {
            Dependency::Version(v) => DetailedDependency {
                version: Some(v.clone()),
                ..Default::default()
            },
            Dependency::Detailed(d) => d.clone(),
        }
    }

    /// Path dependency with no version — fine for local monorepo develop, not publishable.
    pub fn is_path_only(&self) -> bool {
        self.path().is_some() && self.version_req().is_none() && self.git().is_none()
    }

    pub fn to_toml_value(&self) -> toml::Value {
        match self {
            Dependency::Version(v) => toml::Value::String(v.clone()),
            Dependency::Detailed(d) => {
                let mut table = toml::value::Table::new();
                if let Some(v) = &d.version {
                    table.insert("version".into(), toml::Value::String(v.clone()));
                }
                if let Some(v) = &d.registry {
                    table.insert("registry".into(), toml::Value::String(v.clone()));
                }
                if let Some(v) = &d.path {
                    table.insert("path".into(), toml::Value::String(v.clone()));
                }
                if let Some(v) = &d.git {
                    table.insert("git".into(), toml::Value::String(v.clone()));
                }
                if let Some(v) = &d.tag {
                    table.insert("tag".into(), toml::Value::String(v.clone()));
                }
                if let Some(v) = &d.branch {
                    table.insert("branch".into(), toml::Value::String(v.clone()));
                }
                if let Some(v) = &d.rev {
                    table.insert("rev".into(), toml::Value::String(v.clone()));
                }
                toml::Value::Table(table)
            }
        }
    }
}

/// Package names must start with a letter and use only ASCII alphanumerics, `-`, `_`, or `.`.
/// Hyphens and dots in the registry name map to underscores in `import` statements and in
/// `dream_packages/` directory names (`json-tools` / `foo.bar` → `json_tools` / `foo_bar`).
pub fn validate_package_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("package name must not be empty");
    }
    let valid = name.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
    });
    let starts_alpha = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    if !valid || !starts_alpha {
        bail!(
            "invalid package name '{}': must start with a letter and contain only \
             ASCII letters, digits, '-', '_', or '.'",
            name
        );
    }
    Ok(())
}

/// Maps a registry package name to the `import` segment and `dream_packages/` directory name.
/// Hyphens and dots become underscores so `import foo.bar;` keeps its usual subpath meaning
/// (`foo/bar.dream`) and does not collide with a registry package literally named `foo.bar`
/// (`import foo_bar;` after `dreamer add foo.bar`).
pub fn import_segment(package_name: &str) -> String {
    package_name.replace(['-', '.'], "_")
}

/// Reject empty, absolute, or `..`-escaping paths for linked package assets (e.g. `package.icon`).
pub fn validate_relative_asset_path(path: &str, field: &str) -> Result<()> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        bail!("{field} must not be empty");
    }
    let p = Path::new(trimmed);
    if p.is_absolute() {
        bail!("{field} must be relative to the dream.toml directory (got '{trimmed}')");
    }
    for c in p.components() {
        match c {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                bail!("{field} must not contain '..' (got '{trimmed}')");
            }
            _ => bail!("{field} is not a valid relative path (got '{trimmed}')"),
        }
    }
    Ok(())
}

impl Manifest {
    /// Create a `bin` package manifest with the given entry point.
    pub fn new(name: String, version: String, entry: String) -> Self {
        Manifest {
            package: Some(PackageMeta {
                name,
                version,
                package_type: PackageType::Bin,
                edition: None,
                authors: Vec::new(),
                description: None,
                entry: Some(entry),
                license: None,
                keywords: Vec::new(),
                targets: Vec::new(),
                icon: None,
            }),
            workspace: None,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            scripts: BTreeMap::new(),
            registries: BTreeMap::new(),
        }
    }

    /// Create a `lib` package manifest (no entry).
    pub fn new_lib(name: String, version: String) -> Self {
        Manifest {
            package: Some(PackageMeta {
                name,
                version,
                package_type: PackageType::Lib,
                edition: None,
                authors: Vec::new(),
                description: None,
                entry: None,
                license: None,
                keywords: Vec::new(),
                targets: Vec::new(),
                icon: None,
            }),
            workspace: None,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            scripts: BTreeMap::new(),
            registries: BTreeMap::new(),
        }
    }

    /// Virtual workspace root (`[workspace]` only, no `[package]`).
    pub fn new_workspace(members: Vec<String>) -> Self {
        Manifest {
            package: None,
            workspace: Some(WorkspaceMeta { members }),
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            scripts: BTreeMap::new(),
            registries: BTreeMap::new(),
        }
    }

    /// Package metadata; errors if this is a virtual workspace root.
    pub fn package(&self) -> Result<&PackageMeta> {
        self.package.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "this dream.toml is a virtual workspace root (no [package]); pass -p <name> \
                 to select a member"
            )
        })
    }

    pub fn package_mut(&mut self) -> Result<&mut PackageMeta> {
        self.package.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "this dream.toml is a virtual workspace root (no [package]); pass -p <name> \
                 to select a member"
            )
        })
    }

    pub fn is_workspace_root(&self) -> bool {
        self.workspace.is_some()
    }

    pub fn is_virtual_workspace(&self) -> bool {
        self.workspace.is_some() && self.package.is_none()
    }

    pub fn load(path: &Path) -> Result<Manifest> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest at {}", path.display()))?;
        let manifest: Manifest = toml::from_str(&text)
            .with_context(|| format!("parsing manifest at {}", path.display()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.package.is_none() && self.workspace.is_none() {
            bail!("dream.toml must contain [package] and/or [workspace]");
        }
        if let Some(ws) = &self.workspace {
            if ws.members.is_empty() {
                bail!("[workspace].members must list at least one package path");
            }
            for member in &ws.members {
                let trimmed = member.trim();
                if trimmed.is_empty() {
                    bail!("[workspace].members entries must not be empty");
                }
                if Path::new(trimmed).is_absolute() {
                    bail!(
                        "[workspace].members entry '{}' must be relative to the workspace root",
                        trimmed
                    );
                }
                if trimmed.contains('*') || trimmed.contains('?') {
                    bail!(
                        "[workspace].members entry '{}' must be an explicit path (globs are not \
                         supported)",
                        trimmed
                    );
                }
            }
        }
        let Some(pkg) = &self.package else {
            return Ok(());
        };
        validate_package_name(&pkg.name)?;
        Version::parse(&pkg.version).with_context(|| {
            format!(
                "package '{}' has invalid version '{}' (expected semver, e.g. '1.2.3')",
                pkg.name, pkg.version
            )
        })?;
        match pkg.package_type {
            PackageType::Bin => {
                let entry = pkg.entry.as_deref().unwrap_or("").trim();
                if entry.is_empty() {
                    bail!(
                        "package '{}' is type = \"bin\" and requires a non-empty entry \
                         (e.g. entry = \"src/main.dream\")",
                        pkg.name
                    );
                }
            }
            PackageType::Lib => {
                if let Some(entry) = &pkg.entry {
                    if !entry.trim().is_empty() {
                        bail!(
                            "package '{}' is type = \"lib\" and must not set entry \
                             (libraries are imported via src/<name>.dream)",
                            pkg.name
                        );
                    }
                }
            }
        }
        let mut seen = Vec::new();
        for t in &pkg.targets {
            let parsed = RunTarget::parse(t)?;
            if seen.contains(&parsed) {
                bail!(
                    "package.targets lists '{}' more than once",
                    parsed.as_str()
                );
            }
            seen.push(parsed);
        }
        if let Some(icon) = &pkg.icon {
            validate_relative_asset_path(icon, "package.icon")?;
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)
            .with_context(|| format!("serializing manifest for {}", path.display()))?;
        std::fs::write(path, text)
            .with_context(|| format!("writing manifest at {}", path.display()))?;
        Ok(())
    }

    /// Walks upward from `start_dir` looking for the nearest `dream.toml`.
    pub fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = Some(start_dir.to_path_buf());
        while let Some(d) = dir {
            if d.join(MANIFEST_FILE_NAME).is_file() {
                return Some(d);
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        None
    }

    /// Nearest ancestor `dream.toml` that declares `[package]`.
    pub fn find_package_root(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = Some(start_dir.to_path_buf());
        while let Some(d) = dir {
            let candidate = d.join(MANIFEST_FILE_NAME);
            if candidate.is_file() {
                if let Ok(text) = std::fs::read_to_string(&candidate) {
                    if let Ok(m) = toml::from_str::<Manifest>(&text) {
                        if m.package.is_some() {
                            return Some(d);
                        }
                    }
                }
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        None
    }

    /// Nearest ancestor `dream.toml` that declares `[workspace]`.
    pub fn find_workspace_root(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = Some(start_dir.to_path_buf());
        while let Some(d) = dir {
            let candidate = d.join(MANIFEST_FILE_NAME);
            if candidate.is_file() {
                if let Ok(text) = std::fs::read_to_string(&candidate) {
                    if let Ok(m) = toml::from_str::<Manifest>(&text) {
                        if m.workspace.is_some() {
                            return Some(d);
                        }
                    }
                }
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        None
    }

    /// All resolvable dependency entries, optionally including `[dev-dependencies]`.
    pub fn all_dependencies(&self, include_dev: bool) -> BTreeMap<String, Dependency> {
        let mut out = self.dependencies.clone();
        if include_dev {
            out.extend(self.dev_dependencies.clone());
        }
        out
    }

    /// Resolves a dependency's registry alias to a base URL, falling back to `[registries]
    /// default`, then to the built-in default registry.
    pub fn registry_url(&self, alias: Option<&str>) -> Option<String> {
        let alias = alias.unwrap_or("default");
        self.registries
            .get(alias)
            .cloned()
            .or_else(|| (alias == "default").then(|| crate::registry::DEFAULT_REGISTRY.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_package_names() {
        assert!(validate_package_name("json-tools").is_ok());
        assert!(validate_package_name("json_tools").is_ok());
        assert!(validate_package_name("a").is_ok());
        assert!(validate_package_name("").is_err());
        assert!(validate_package_name("1abc").is_err());
        assert!(validate_package_name("has space").is_err());
        assert!(validate_package_name("has.dot").is_ok());
    }

    #[test]
    fn maps_hyphens_and_dots_to_underscores_for_import_segments() {
        assert_eq!(import_segment("json-tools"), "json_tools");
        assert_eq!(import_segment("foo.bar"), "foo_bar");
        assert_eq!(import_segment("json-tools.extra"), "json_tools_extra");
        assert_eq!(import_segment("already_underscored"), "already_underscored");
    }

    #[test]
    fn round_trips_through_toml() {
        let mut manifest = Manifest::new(
            "myapp".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest.dependencies.insert(
            "json-tools".to_string(),
            Dependency::Version("^0.3".to_string()),
        );
        manifest.dependencies.insert(
            "local-lib".to_string(),
            Dependency::Detailed(DetailedDependency {
                path: Some("../local-lib".to_string()),
                ..Default::default()
            }),
        );

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(MANIFEST_FILE_NAME);
        manifest.save(&path).unwrap();

        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.package().unwrap().name, "myapp");
        assert_eq!(loaded.package().unwrap().package_type, PackageType::Bin);
        assert_eq!(
            loaded.package().unwrap().entry.as_deref(),
            Some("src/main.dream")
        );
        assert_eq!(loaded.dependencies.len(), 2);
        assert_eq!(
            loaded.dependencies.get("json-tools").unwrap().version_req(),
            Some("^0.3")
        );
        assert_eq!(
            loaded.dependencies.get("local-lib").unwrap().path(),
            Some("../local-lib")
        );
    }

    #[test]
    fn rejects_invalid_semver_on_validate() {
        let manifest = Manifest::new(
            "myapp".to_string(),
            "not-a-version".to_string(),
            "src/main.dream".to_string(),
        );
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn icon_round_trip_and_path_rules() {
        let mut manifest = Manifest::new(
            "myapp".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest.package_mut().unwrap().icon = Some("assets/icon.png".into());
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(MANIFEST_FILE_NAME);
        manifest.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(
            loaded.package().unwrap().icon.as_deref(),
            Some("assets/icon.png")
        );

        manifest.package_mut().unwrap().icon = Some("../escape.png".into());
        assert!(manifest.validate().is_err());
        manifest.package_mut().unwrap().icon = Some("/abs/icon.png".into());
        assert!(manifest.validate().is_err());
        manifest.package_mut().unwrap().icon = Some("".into());
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_unknown_package_targets() {
        let mut manifest = Manifest::new(
            "myapp".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest.package_mut().unwrap().targets = vec!["browser".to_string()];
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn round_trips_package_targets() {
        let mut manifest = Manifest::new(
            "myapp".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest.package_mut().unwrap().targets = vec!["native".into(), "web".into()];
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(MANIFEST_FILE_NAME);
        manifest.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.package().unwrap().targets, vec!["native", "web"]);
    }

    #[test]
    fn lib_rejects_entry_and_bin_requires_it() {
        let mut lib = Manifest::new_lib("http-utils".into(), "0.1.0".into());
        assert!(lib.validate().is_ok());
        lib.package_mut().unwrap().entry = Some("src/main.dream".into());
        assert!(lib.validate().is_err());

        let mut bin = Manifest::new("myapp".into(), "0.1.0".into(), "src/main.dream".into());
        assert!(bin.validate().is_ok());
        bin.package_mut().unwrap().entry = None;
        assert!(bin.validate().is_err());
        bin.package_mut().unwrap().entry = Some(String::new());
        assert!(bin.validate().is_err());
    }

    #[test]
    fn round_trips_lib_without_entry() {
        let manifest = Manifest::new_lib("http-utils".into(), "0.1.0".into());
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(MANIFEST_FILE_NAME);
        manifest.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.package().unwrap().package_type, PackageType::Lib);
        assert!(loaded.package().unwrap().entry.is_none());
    }

    #[test]
    fn resolve_run_target_empty_defaults_to_native() {
        assert_eq!(resolve_run_target(&[], None).unwrap(), RunTarget::Native);
        assert_eq!(
            resolve_run_target(&[], Some("web")).unwrap(),
            RunTarget::Web
        );
    }

    #[test]
    fn resolve_run_target_single_auto_selects() {
        assert_eq!(
            resolve_run_target(&["node".into()], None).unwrap(),
            RunTarget::Node
        );
        assert!(resolve_run_target(&["node".into()], Some("web")).is_err());
    }

    #[test]
    fn resolve_run_target_multi_requires_explicit() {
        let targets = vec!["native".into(), "web".into()];
        assert!(resolve_run_target(&targets, None).is_err());
        assert_eq!(
            resolve_run_target(&targets, Some("web")).unwrap(),
            RunTarget::Web
        );
        assert!(resolve_run_target(&targets, Some("node")).is_err());
    }

    #[test]
    fn finds_project_root_by_walking_upward() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = Manifest::new(
            "myapp".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest.save(&tmp.path().join(MANIFEST_FILE_NAME)).unwrap();

        let nested = tmp.path().join("src").join("deep").join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        assert_eq!(
            Manifest::find_project_root(&nested),
            Some(tmp.path().to_path_buf())
        );
    }

    #[test]
    fn no_project_root_found_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(Manifest::find_project_root(tmp.path()), None);
    }

    #[test]
    fn round_trips_virtual_workspace() {
        let manifest = Manifest::new_workspace(vec![
            "packages/shared".into(),
            "apps/cli".into(),
        ]);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(MANIFEST_FILE_NAME);
        manifest.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert!(loaded.is_virtual_workspace());
        assert_eq!(
            loaded.workspace.as_ref().unwrap().members,
            vec!["packages/shared", "apps/cli"]
        );
        assert!(loaded.package().is_err());
    }

    #[test]
    fn finds_package_root_skipping_virtual_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        Manifest::new_workspace(vec!["apps/cli".into()])
            .save(&tmp.path().join(MANIFEST_FILE_NAME))
            .unwrap();
        let cli = tmp.path().join("apps").join("cli");
        std::fs::create_dir_all(cli.join("src")).unwrap();
        Manifest::new("cli".into(), "0.1.0".into(), "src/main.dream".into())
            .save(&cli.join(MANIFEST_FILE_NAME))
            .unwrap();
        let nested = cli.join("src");
        assert_eq!(Manifest::find_package_root(&nested), Some(cli));
        assert_eq!(
            Manifest::find_workspace_root(&nested),
            Some(tmp.path().to_path_buf())
        );
    }
}
