//! Native C/C++ source sets: every package's `native/` directory (the implicit set named after the
//! package) plus explicit `[native.<set>]` tables, resolved across the entry project and its
//! `dream_packages/`. Also fills in bare `@c` / `@c("lib")` so everything downstream sees an
//! explicit `@c("lib", "symbol")`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{AttributeArg, AttributeNode, FunctionNode};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

use crate::driver::project_manifest::{
    find_project_root_from, import_segment, NativeTable, ProjectManifest, MANIFEST_FILE_NAME,
};
use crate::driver::source_loader::{find_dream_packages_dir, ProgramAccumulator};

pub const C_EXTENSIONS: [&str; 1] = ["c"];
pub const CXX_EXTENSIONS: [&str; 3] = ["cpp", "cc", "cxx"];
pub const WASM_C_LIBRARIES: &[&str] = &["c", "m"];

/// One resolved set, with absolute paths, ready to compile on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSet {
    pub name: String,
    /// Directory holding the declaring `dream.toml`.
    pub root: PathBuf,
    pub sources: Vec<PathBuf>,
    pub include: Vec<PathBuf>,
    pub defines: Vec<String>,
    pub cflags: Vec<String>,
    pub frameworks: Vec<String>,
    pub libs: Vec<String>,
}

impl NativeSet {
    pub fn has_cxx(&self) -> bool {
        self.sources.iter().any(|s| is_cxx_source(s))
    }
}

pub fn is_cxx_source(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CXX_EXTENSIONS.contains(&e))
}

fn is_native_source(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| C_EXTENSIONS.contains(&e) || CXX_EXTENSIONS.contains(&e))
}

/// Every set visible to one compile, plus which set each package root declares implicitly.
#[derive(Debug, Default)]
pub struct NativeGraph {
    pub sets: BTreeMap<String, NativeSet>,
    /// Package root → its implicit (`native/`) set name, used to resolve bare `@c`.
    pub implicit: BTreeMap<PathBuf, String>,
    /// Source tags that stand for an on-disk file (the LSP analyzes its document as `main.dream`).
    pub aliases: BTreeMap<String, String>,
}

impl NativeGraph {
    /// Loads sets from the entry project, every installed `dream_packages/*` package, and the
    /// package root of every loaded source file. Duplicate set names and duplicate
    /// `[package] links` values are errors naming both manifests.
    pub fn load(
        entry_file: &str,
        acc: &ProgramAccumulator<'_>,
        target: &dream_abi::target::TargetSpec,
    ) -> Result<Self, String> {
        let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
        let entry = canonical(Path::new(entry_file));
        if let Some(root) = entry.parent().and_then(find_project_root_from) {
            roots.insert(canonical(&root));
            if let Some(pkgs) = find_dream_packages_dir(&root)
                && let Ok(rd) = std::fs::read_dir(&pkgs) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.join(MANIFEST_FILE_NAME).is_file() {
                            roots.insert(canonical(&p));
                        }
                    }
                }
        }
        for file in acc.file_contents.keys() {
            let path = Path::new(file);
            if !path.is_absolute() || !path.is_file() {
                continue;
            }
            if let Some(root) = path.parent().and_then(find_project_root_from) {
                roots.insert(canonical(&root));
            }
        }
        let mut graph = NativeGraph::default();
        let mut set_owner: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut links_owner: BTreeMap<String, PathBuf> = BTreeMap::new();
        for root in roots {
            let manifest = ProjectManifest::load(&root)?;
            let manifest_path = root.join(MANIFEST_FILE_NAME);
            if let Some(links) = &manifest.links
                && let Some(prev) = links_owner.insert(links.clone(), manifest_path.clone()) {
                    return Err(format!(
                        "native library '{links}' is provided by two packages: {} and {} \
                         (`[package] links` must be unique across the dependency graph)",
                        prev.display(),
                        manifest_path.display()
                    ));
                }
            for set in sets_for_package(&root, &manifest, target)? {
                if let Some(prev) = set_owner.insert(set.name.clone(), manifest_path.clone()) {
                    return Err(format!(
                        "native set '{}' is declared by two packages: {} and {}",
                        set.name,
                        prev.display(),
                        manifest_path.display()
                    ));
                }
                graph.sets.insert(set.name.clone(), set);
            }
            if let Some(name) = implicit_set_name(&root, &manifest)
                && graph.sets.contains_key(&name) {
                    graph.implicit.insert(root.clone(), name);
                }
        }
        Ok(graph)
    }

    /// The set a bare `@c` in `file` binds to: the implicit set of its package.
    pub fn implicit_set_for_file(&self, file: &str) -> Option<&str> {
        let file = self.aliases.get(file).map_or(file, String::as_str);
        let root = canonical(Path::new(file))
            .parent()
            .and_then(find_project_root_from)?;
        self.implicit.get(&canonical(&root)).map(String::as_str)
    }
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn implicit_set_name(root: &Path, manifest: &ProjectManifest) -> Option<String> {
    if !root.join("native").is_dir() {
        return None;
    }
    let name = manifest
        .package_name
        .clone()
        .or_else(|| root.file_name()?.to_str().map(str::to_string))?;
    Some(import_segment(&name))
}

/// The implicit `native/` set (when it holds sources) merged with a same-named table, then every
/// other `[native.<set>]` table.
fn sets_for_package(
    root: &Path,
    manifest: &ProjectManifest,
    target: &dream_abi::target::TargetSpec,
) -> Result<Vec<NativeSet>, String> {
    let mut out = Vec::new();
    let implicit = implicit_set_name(root, manifest);
    if let Some(name) = &implicit {
        let native_dir = root.join("native");
        let mut sources = Vec::new();
        walk_sources(&native_dir, &mut sources);
        let extra = manifest
            .native
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.for_target(target))
            .unwrap_or_default();
        let mut include = Vec::new();
        if native_dir.join("include").is_dir() {
            include.push(native_dir.join("include"));
        }
        include.push(native_dir.clone());
        let set = build_set(root, name, sources, include, &extra)?;
        if !set.sources.is_empty() {
            out.push(set);
        }
    }
    for (name, spec) in &manifest.native {
        if implicit.as_deref() == Some(name.as_str()) {
            continue;
        }
        let table = spec.for_target(target);
        let set = build_set(root, name, Vec::new(), Vec::new(), &table)?;
        if set.sources.is_empty() {
            return Err(format!(
                "[native.{name}] in {} has no C/C++ sources on this host (add `sources = [...]`)",
                root.join(MANIFEST_FILE_NAME).display()
            ));
        }
        out.push(set);
    }
    Ok(out)
}

fn build_set(
    root: &Path,
    name: &str,
    mut sources: Vec<PathBuf>,
    mut include: Vec<PathBuf>,
    table: &NativeTable,
) -> Result<NativeSet, String> {
    for rel in &table.sources {
        let path = root.join(rel);
        if path.is_dir() {
            walk_sources(&path, &mut sources);
        } else if path.is_file() {
            sources.push(path);
        } else {
            return Err(format!(
                "[native.{name}] source '{rel}' does not exist under {}",
                root.display()
            ));
        }
    }
    for rel in &table.include {
        include.push(root.join(rel));
    }
    let mut seen = BTreeSet::new();
    sources.retain(|s| seen.insert(canonical(s)));
    sources.sort();
    Ok(NativeSet {
        name: name.to_string(),
        root: root.to_path_buf(),
        sources: sources.iter().map(|s| canonical(s)).collect(),
        include: include.iter().map(|p| canonical(p)).collect(),
        defines: table.defines.clone(),
        cflags: table.cflags.clone(),
        frameworks: table.frameworks.clone(),
        libs: table.libs.clone(),
    })
}

fn walk_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk_sources(&p, out);
        } else if is_native_source(&p) {
            out.push(p);
        }
    }
}

/// Rewrites `@c` / `@c("lib")` on every extern into `@c("lib", "symbol")`: the library defaults to
/// the declaring package's implicit native set and the symbol to the Dream function name.
pub fn resolve_bare_c_attrs(
    acc: &mut ProgramAccumulator<'_>,
    graph: &NativeGraph,
    diagnostics: &mut DiagnosticBag,
) {
    let functions = acc.all_functions.iter_mut();
    let methods = acc
        .all_structs
        .iter_mut()
        .flat_map(|s| s.methods.iter_mut());
    let extends = acc
        .all_extends
        .iter_mut()
        .flat_map(|e| e.methods.iter_mut());
    for f in functions.chain(methods).chain(extends) {
        resolve_one(f, graph, diagnostics);
    }
}

fn resolve_one(f: &mut FunctionNode<'_>, graph: &NativeGraph, diagnostics: &mut DiagnosticBag) {
    let file = f.file_path.clone();
    let name = f.name.text.clone();
    let Some(attr) = f.attributes.iter_mut().find(|a| a.name.text == "c") else {
        return;
    };
    if attr.args.len() >= 2 || attr.args.iter().any(|a| a.as_string().is_none()) {
        return;
    }
    if attr.args.is_empty() {
        let set = file.as_deref().and_then(|f| graph.implicit_set_for_file(f));
        let Some(set) = set else {
            diagnostics.report_error(
                format!(
                    "bare `@c` on '{name}' needs C/C++ sources in a `native/` directory next to \
                     the declaring package's dream.toml; name the library explicitly with \
                     `@c(\"lib\", \"symbol\")` otherwise"
                ),
                Some(attr.name.position),
            );
            return;
        };
        attr.args.push(string_arg(attr, set));
    }
    attr.args.push(string_arg(attr, &name));
}

fn string_arg(attr: &AttributeNode, value: &str) -> AttributeArg {
    AttributeArg::String(SyntaxToken::new(
        TokenKind::StringToken,
        attr.name.position,
        format!("\"{value}\""),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dream-native-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn load(root: &Path) -> Result<NativeGraph, String> {
        let entry = root.join("src/main.dream");
        NativeGraph::load(
            entry.to_str().unwrap(),
            &ProgramAccumulator::default(),
            &dream_abi::target::TargetSpec::host(),
        )
    }

    #[test]
    fn discovers_implicit_set_and_merges_extras() {
        let root = temp_root("implicit");
        write(
            &root.join("dream.toml"),
            "[package]\nname = \"kv-store\"\n\n[native.kv_store]\ndefines = [\"A=1\"]\n",
        );
        write(&root.join("src/main.dream"), "fun main(): void {}\n");
        write(&root.join("native/include/kv.hpp"), "");
        write(&root.join("native/kv.cpp"), "");
        write(&root.join("native/vendor/lmdb.c"), "");
        let graph = load(&root).unwrap();
        let set = graph.sets.get("kv_store").unwrap();
        let names: Vec<_> = set
            .sources
            .iter()
            .map(|s| s.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["kv.cpp", "lmdb.c"]);
        assert!(set.has_cxx());
        assert_eq!(set.defines, vec!["A=1"]);
        assert!(set.include[0].ends_with("native/include"));
        let file = root.join("src/main.dream");
        assert_eq!(
            graph.implicit_set_for_file(file.to_str().unwrap()),
            Some("kv_store")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn explicit_set_without_sources_is_an_error() {
        let root = temp_root("empty");
        write(
            &root.join("dream.toml"),
            "[package]\nname = \"x\"\n\n[native.extra]\ncflags = [\"-O2\"]\n",
        );
        let err = load(&root).unwrap_err();
        assert!(err.contains("has no C/C++ sources"), "{}", err);
        write(
            &root.join("dream.toml"),
            "[package]\nname = \"x\"\n\n[native.extra]\nsources = [\"missing.c\"]\n",
        );
        let err = load(&root).unwrap_err();
        assert!(err.contains("does not exist"), "{}", err);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicate_links_across_packages_is_an_error() {
        let root = temp_root("links");
        write(&root.join("dream.toml"), "[package]\nname = \"app\"\n");
        for pkg in ["a", "b"] {
            write(
                &root.join(format!("dream_packages/{pkg}/dream.toml")),
                &format!("[package]\nname = \"{pkg}\"\nlinks = \"sqlite3\"\n"),
            );
        }
        let err = load(&root).unwrap_err();
        assert!(
            err.contains("native library 'sqlite3' is provided by two packages"),
            "{}",
            err
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicate_set_names_across_packages_is_an_error() {
        let root = temp_root("dupe");
        write(&root.join("dream.toml"), "[package]\nname = \"app\"\n");
        for pkg in ["a", "b"] {
            write(
                &root.join(format!("dream_packages/{pkg}/dream.toml")),
                &format!("[package]\nname = \"{pkg}\"\n\n[native.shared]\nsources = [\"x.c\"]\n"),
            );
            write(&root.join(format!("dream_packages/{pkg}/x.c")), "");
        }
        let err = load(&root).unwrap_err();
        assert!(
            err.contains("native set 'shared' is declared by two packages"),
            "{}",
            err
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
