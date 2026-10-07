//! Dependency resolution: turns a manifest's `[dependencies]`/`[dev-dependencies]` into a
//! concrete, versioned dependency graph.
//!
//! Registry dependencies use a greedy "highest version satisfying every accumulated requirement"
//! strategy — the same class of algorithm as Cargo's classic (non-`-Zminimal-versions`) resolver:
//! requirements accumulate breadth-first (a package's own registry dependencies contribute new
//! requirements once *it* is resolved), and the loop repeats until a fixed point is reached. This
//! intentionally does not attempt full PubGrub-style backtracking; diamond dependencies resolve
//! correctly as long as a single version can satisfy every accumulated requirement, and conflicts
//! are reported clearly rather than silently picked around.
//!
//! Path and git dependencies are resolved immediately by reading the dependency's own
//! `dream.toml` and are treated as pinned (never subject to registry version selection).

use crate::manifest::{Dependency, MANIFEST_FILE_NAME, Manifest};
use crate::registry::{IndexEntry, open_registry};
use anyhow::{Context, Result, anyhow, bail};
use semver::{Version, VersionReq};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum ResolvedSource {
    Registry {
        url: String,
        tarball: String,
        checksum: String,
    },
    Path {
        path: PathBuf,
    },
    Git {
        url: String,
        checkout_dir: PathBuf,
        commit: String,
        selector: String,
    },
}

impl ResolvedSource {
    /// The `source` string recorded in `dream.lock`.
    pub fn lock_source(&self) -> String {
        match self {
            ResolvedSource::Registry { url, .. } => format!("registry+{}", url),
            ResolvedSource::Path { path } => format!("path+{}", path.display()),
            ResolvedSource::Git { url, commit, .. } => format!("git+{url}#{commit}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub source: ResolvedSource,
    /// Names of this package's own dependencies (versions filled in by the caller once the whole
    /// graph is known).
    pub dependencies: Vec<String>,
}

/// Resolves `manifest`'s dependency graph. `preferred` is an optional map of package name ->
/// version (typically sourced from an existing `dream.lock`): when a registry package has
/// multiple versions satisfying every accumulated requirement, the preferred version is kept
/// instead of jumping to the newest match, so re-running `install` doesn't silently upgrade
/// dependencies just because a newer version was published.
pub fn resolve(
    manifest: &Manifest,
    project_dir: &Path,
    include_dev: bool,
    preferred: &BTreeMap<String, crate::lockfile::LockedPackage>,
) -> Result<Vec<ResolvedPackage>> {
    resolve_many(
        &[(project_dir.to_path_buf(), manifest.clone())],
        include_dev,
        preferred,
    )
}

/// Union-resolve dependencies from every `(dir, manifest)` pair (workspace members). Path deps
/// are resolved relative to each member's directory; registry requirements accumulate globally.
pub fn resolve_many(
    members: &[(PathBuf, Manifest)],
    include_dev: bool,
    preferred: &BTreeMap<String, crate::lockfile::LockedPackage>,
) -> Result<Vec<ResolvedPackage>> {
    let mut resolver = Resolver {
        preferred: preferred.clone(),
        ..Resolver::default()
    };
    for (dir, manifest) in members {
        for (name, dep) in manifest.all_dependencies(include_dev) {
            resolver.queue_dependency(&name, &dep, dir, manifest)?;
        }
    }
    resolver.run()?;
    Ok(resolver.resolved.into_values().collect())
}

#[derive(Default)]
struct Resolver {
    resolved: BTreeMap<String, ResolvedPackage>,
    requirements: BTreeMap<String, Vec<(VersionReq, String)>>,
    index_cache: HashMap<(String, String), Vec<IndexEntry>>,
    visited_dirs: HashSet<PathBuf>,
    preferred: BTreeMap<String, crate::lockfile::LockedPackage>,
    sources: BTreeMap<String, String>,
}

impl Resolver {
    fn claim_source(&mut self, name: &str, source: &str) -> Result<()> {
        crate::manifest::validate_package_name(name)?;
        if let Some(previous) = self.sources.get(name) {
            if previous != source {
                bail!("conflicting package sources for '{name}': {previous} versus {source}");
            }
        } else {
            let segment = crate::manifest::import_segment(name);
            if let Some(other) = self
                .sources
                .keys()
                .find(|other| crate::manifest::import_segment(other) == segment)
            {
                bail!("package import names collide: '{name}' and '{other}'");
            }
            self.sources.insert(name.to_string(), source.to_string());
        }
        Ok(())
    }

    fn queue_dependency(
        &mut self,
        name: &str,
        dep: &Dependency,
        base_dir: &Path,
        manifest: &Manifest,
    ) -> Result<()> {
        crate::manifest::validate_package_name(name)?;
        dep.validate()?;
        if let Some(rel_path) = dep.path() {
            let dep_dir = base_dir.join(rel_path).canonicalize().with_context(|| {
                format!("resolving path dependency '{}' at '{}'", name, rel_path)
            })?;
            self.claim_source(name, &format!("path+{}", dep_dir.display()))?;
            self.queue_local_project(name, dep_dir, ResolvedKind::Path)?;
            return Ok(());
        }

        if let Some(git_url) = dep.git() {
            let d = dep.detailed();
            self.claim_source(
                name,
                &format!(
                    "git+{}#{:?}",
                    git_url,
                    (d.tag.as_deref(), d.branch.as_deref(), d.rev.as_deref())
                ),
            )?;
            let selector = d
                .tag
                .as_ref()
                .map(|v| format!("refs/tags/{v}"))
                .or_else(|| d.branch.as_ref().map(|v| format!("refs/heads/{v}")))
                .or_else(|| d.rev.clone())
                .unwrap_or_else(|| "HEAD".to_string());
            let locked = self
                .preferred
                .get(name)
                .filter(|p| p.git_selector.as_deref() == Some(&selector));
            let prefix = format!("git+{git_url}#");
            let pinned = locked.and_then(|p| p.source.strip_prefix(&prefix));
            let checkout = crate::git::fetch_git_dependency(git_url, &selector, pinned)?;
            self.queue_local_project(
                name,
                checkout.path,
                ResolvedKind::Git {
                    url: git_url.to_string(),
                    commit: checkout.commit,
                    selector,
                },
            )?;
            return Ok(());
        }

        let req_str = dep.version_req().unwrap_or("*");
        let req = VersionReq::parse(req_str)
            .with_context(|| format!("parsing version requirement '{}' for '{}'", req_str, name))?;
        let url = manifest
            .registry_url(dep.registry_alias())
            .ok_or_else(|| anyhow!("no registry configured for dependency '{}'", name))?;
        self.claim_source(name, &format!("registry+{url}"))?;
        self.requirements
            .entry(name.to_string())
            .or_default()
            .push((req, url));
        Ok(())
    }

    fn queue_local_project(
        &mut self,
        expected_name: &str,
        dir: PathBuf,
        kind: ResolvedKind,
    ) -> Result<()> {
        if !self.visited_dirs.insert(dir.clone()) {
            return Ok(());
        }
        let manifest_path = dir.join(MANIFEST_FILE_NAME);
        let dep_manifest = Manifest::load(&manifest_path).with_context(|| {
            format!("loading manifest for local dependency at {}", dir.display())
        })?;

        let source = match kind {
            ResolvedKind::Path => ResolvedSource::Path { path: dir.clone() },
            ResolvedKind::Git {
                url,
                commit,
                selector,
            } => ResolvedSource::Git {
                url,
                checkout_dir: dir.clone(),
                commit,
                selector,
            },
        };

        let child_deps = dep_manifest.all_dependencies(false);
        let pkg_name = dep_manifest.package()?.name.clone();
        if pkg_name != expected_name {
            bail!("dependency '{expected_name}' contains package '{pkg_name}'");
        }
        let pkg_version = dep_manifest.package()?.version.clone();
        self.resolved.insert(
            pkg_name.clone(),
            ResolvedPackage {
                name: pkg_name,
                version: pkg_version,
                source,
                dependencies: child_deps.keys().cloned().collect(),
            },
        );
        for (child_name, child_dep) in child_deps {
            self.queue_dependency(&child_name, &child_dep, &dir, &dep_manifest)?;
        }
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        loop {
            let mut changed = false;
            let names: Vec<String> = self.requirements.keys().cloned().collect();

            for name in names {
                if self.resolved.contains_key(&name) {
                    // Already pinned by a path/git dependency; registry requirements on the same
                    // name are intentionally overridden (documented pinning behavior).
                    continue;
                }

                let reqs = self.requirements.get(&name).cloned().unwrap_or_default();
                let Some((_, url)) = reqs.first() else {
                    continue;
                };
                let url = url.clone();

                let cache_key = (url.clone(), name.clone());
                let entries = match self.index_cache.get(&cache_key) {
                    Some(e) => e.clone(),
                    None => {
                        let client = open_registry(&url);
                        let entries = client.fetch_index(&name).with_context(|| {
                            format!("fetching index for '{}' from {}", name, url)
                        })?;
                        self.index_cache.insert(cache_key, entries.clone());
                        entries
                    }
                };

                for entry in &entries {
                    crate::manifest::validate_package_name(&entry.name)?;
                    if entry.name != name { bail!("registry index contains a different package identity"); }
                    let digest = entry.cksum.strip_prefix("sha256:").unwrap_or("");
                    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                        bail!("registry package has an invalid checksum");
                    }
                }

                if entries.is_empty() {
                    bail!("no package named '{}' found in registry {}", name, url);
                }

                let mut candidates: Vec<&IndexEntry> = entries
                    .iter()
                    .filter(|e| {
                        Version::parse(&e.vers)
                            .map(|v| reqs.iter().all(|(r, _)| r.matches(&v)))
                            .unwrap_or(false)
                    })
                    .collect();
                candidates.sort_by_key(|e| Version::parse(&e.vers).unwrap());
                let preferred_match = self
                    .preferred
                    .get(&name)
                    .and_then(|v| {
                        candidates
                            .iter()
                            .find(|e| e.vers == v.version && v.source == format!("registry+{url}"))
                    })
                    .copied();
                let Some(chosen) = preferred_match.or_else(|| candidates.last().copied()) else {
                    let reqs_str = reqs
                        .iter()
                        .map(|(r, _)| r.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    bail!(
                        "no version of '{}' satisfies all requirements: {}",
                        name,
                        reqs_str
                    );
                };

                let already_current = self
                    .resolved
                    .get(&name)
                    .is_some_and(|p| p.version == chosen.vers);
                if already_current {
                    continue;
                }

                changed = true;
                self.resolved.insert(
                    name.clone(),
                    ResolvedPackage {
                        name: name.clone(),
                        version: chosen.vers.clone(),
                        source: ResolvedSource::Registry {
                            url: url.clone(),
                            tarball: chosen.tarball.clone(),
                            checksum: chosen.cksum.clone(),
                        },
                        dependencies: chosen.deps.iter().map(|d| d.name.clone()).collect(),
                    },
                );
                for dep in &chosen.deps {
                    let req = VersionReq::parse(&dep.req).with_context(|| {
                        format!(
                            "parsing version requirement '{}' declared by {} {} for '{}'",
                            dep.req, name, chosen.vers, dep.name
                        )
                    })?;
                    self.claim_source(&dep.name, &format!("registry+{url}"))?;
                    self.requirements
                        .entry(dep.name.clone())
                        .or_default()
                        .push((req, url.clone()));
                }
            }

            if !changed {
                break;
            }
        }
        Ok(())
    }
}

enum ResolvedKind {
    Path,
    Git {
        url: String,
        commit: String,
        selector: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{MANIFEST_FILE_NAME, Manifest};
    use crate::registry::{IndexDependency, IndexEntry};
    use std::collections::BTreeMap;

    fn publish(registry_dir: &Path, name: &str, vers: &str, deps: &[(&str, &str)]) {
        let registry =
            crate::registry::open_registry(&format!("file://{}", registry_dir.display()));
        let tarball_src = registry_dir.join(format!("staging-{}-{}.tar.gz", name, vers));
        std::fs::write(&tarball_src, b"unused in resolver tests").unwrap();
        let entry = IndexEntry {
            name: name.to_string(),
            vers: vers.to_string(),
            deps: deps
                .iter()
                .map(|(n, r)| IndexDependency {
                    name: n.to_string(),
                    req: r.to_string(),
                })
                .collect(),
            cksum: crate::registry::checksum::sha256_of(b"unused in resolver tests"),
            tarball: format!("dl/{}/{}-{}.tar.gz", name, name, vers),
            ..Default::default()
        };
        registry.publish(&entry, &tarball_src).unwrap();
    }

    fn manifest_with_deps(deps: &[(&str, &str)], registry_url: &str) -> Manifest {
        let mut manifest = Manifest::new(
            "app".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest
            .registries
            .insert("default".to_string(), registry_url.to_string());
        for (name, req) in deps {
            manifest.dependencies.insert(
                name.to_string(),
                crate::manifest::Dependency::Version(req.to_string()),
            );
        }
        manifest
    }

    #[test]
    fn resolves_diamond_dependency_to_a_single_compatible_version() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_url = format!("file://{}", tmp.path().display());

        // app -> a (^1.0), app -> b (^1.0); a -> shared (^1.0), b -> shared (^1.1) => shared 1.1.0
        publish(tmp.path(), "shared", "1.0.0", &[]);
        publish(tmp.path(), "shared", "1.1.0", &[]);
        publish(tmp.path(), "a", "1.0.0", &[("shared", "^1.0")]);
        publish(tmp.path(), "b", "1.0.0", &[("shared", "^1.1")]);

        let manifest = manifest_with_deps(&[("a", "^1.0"), ("b", "^1.0")], &registry_url);
        let resolved = resolve(&manifest, tmp.path(), false, &BTreeMap::new()).unwrap();

        let shared = resolved.iter().find(|p| p.name == "shared").unwrap();
        assert_eq!(shared.version, "1.1.0");
        assert_eq!(resolved.len(), 3);
    }

    #[test]
    fn errors_when_no_version_satisfies_every_requirement() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_url = format!("file://{}", tmp.path().display());

        publish(tmp.path(), "shared", "1.0.0", &[]);
        publish(tmp.path(), "shared", "2.0.0", &[]);
        publish(tmp.path(), "a", "1.0.0", &[("shared", "^1.0")]);
        publish(tmp.path(), "b", "1.0.0", &[("shared", "^2.0")]);

        let manifest = manifest_with_deps(&[("a", "^1.0"), ("b", "^1.0")], &registry_url);
        let err = resolve(&manifest, tmp.path(), false, &BTreeMap::new()).unwrap_err();
        assert!(err.to_string().contains("no version of 'shared' satisfies"));
    }

    #[test]
    fn prefers_locked_version_when_still_compatible() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_url = format!("file://{}", tmp.path().display());
        publish(tmp.path(), "pkg", "1.0.0", &[]);
        publish(tmp.path(), "pkg", "1.2.0", &[]);

        let manifest = manifest_with_deps(&[("pkg", "^1.0")], &registry_url);

        let mut preferred = BTreeMap::new();
        preferred.insert(
            "pkg".to_string(),
            crate::lockfile::LockedPackage {
                name: "pkg".to_string(),
                version: "1.0.0".to_string(),
                source: format!("registry+{registry_url}"),
                checksum: None,
                git_selector: None,
                dependencies: Vec::new(),
            },
        );
        let resolved = resolve(&manifest, tmp.path(), false, &preferred).unwrap();
        assert_eq!(resolved[0].version, "1.0.0");

        // Without a preference, the newest compatible version wins.
        let resolved = resolve(&manifest, tmp.path(), false, &BTreeMap::new()).unwrap();
        assert_eq!(resolved[0].version, "1.2.0");
    }

    #[test]
    fn resolves_path_dependency_from_its_own_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let lib_dir = tmp.path().join("local-lib");
        std::fs::create_dir_all(lib_dir.join("src")).unwrap();
        let lib_manifest = Manifest::new_lib("local-lib".to_string(), "0.2.0".to_string());
        lib_manifest
            .save(&lib_dir.join(MANIFEST_FILE_NAME))
            .unwrap();
        std::fs::write(
            lib_dir.join("src").join("local_lib.dream"),
            "public fun answer(): int { return 42; }\n",
        )
        .unwrap();

        let app_dir = tmp.path().join("app");
        std::fs::create_dir_all(&app_dir).unwrap();
        let mut manifest = Manifest::new(
            "app".to_string(),
            "0.1.0".to_string(),
            "src/main.dream".to_string(),
        );
        manifest.dependencies.insert(
            "local-lib".to_string(),
            crate::manifest::Dependency::Detailed(crate::manifest::DetailedDependency {
                path: Some("../local-lib".to_string()),
                ..Default::default()
            }),
        );

        let resolved = resolve(&manifest, &app_dir, false, &BTreeMap::new()).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "local-lib");
        assert_eq!(resolved[0].version, "0.2.0");
        assert!(matches!(resolved[0].source, ResolvedSource::Path { .. }));
    }
}
