//! `dream.toml` as the compiler reads it: `[package]` (name, entry, links), `[lib].output-type`, `[[generators]]`, and
//! `[native.<set>]` tables. Parsed with `toml`; `dreamer`'s own `Manifest` owns validation of the
//! package-manager keys, so unknown top-level tables are ignored here.

use std::path::{Path, PathBuf};

pub const MANIFEST_FILE_NAME: &str = "dream.toml";

/// Per-set native build settings (a `[native.<set>]` table or one of its OS subtables).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeTable {
    pub sources: Vec<String>,
    pub include: Vec<String>,
    pub defines: Vec<String>,
    pub cflags: Vec<String>,
    pub frameworks: Vec<String>,
    pub libs: Vec<String>,
}

/// One `[native.<set>]` table: shared settings plus `macos` / `linux` / `windows` overlays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeSetSpec {
    pub base: NativeTable,
    pub macos: NativeTable,
    pub linux: NativeTable,
    pub windows: NativeTable,
    pub wasm: NativeTable,
}

impl NativeSetSpec {
    /// Shared settings plus the build target's overlay, independent of the host OS.
    pub fn for_target(&self, target: &dream_abi::target::TargetSpec) -> NativeTable {
        let os = if target.capabilities.linear_memory {
            &self.wasm
        } else if target.is_windows() {
            &self.windows
        } else if target.is_apple() {
            &self.macos
        } else {
            &self.linux
        };
        let mut t = self.base.clone();
        t.sources.extend(os.sources.iter().cloned());
        t.include.extend(os.include.iter().cloned());
        t.defines.extend(os.defines.iter().cloned());
        t.cflags.extend(os.cflags.iter().cloned());
        t.frameworks.extend(os.frameworks.iter().cloned());
        t.libs.extend(os.libs.iter().cloned());
        t
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProjectManifest {
    pub library: Option<dream_abi::library::LibraryConfig>,
    pub package_name: Option<String>,
    pub entry: Option<String>,
    pub links: Option<String>,
    pub generators: Vec<String>,
    /// `[native.<set>]` tables in sorted set-name order.
    pub native: Vec<(String, NativeSetSpec)>,
}

impl ProjectManifest {
    pub fn load(project_root: &Path) -> Result<Self, String> {
        let path = project_root.join(MANIFEST_FILE_NAME);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let root: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let mut m = ProjectManifest::default();
        if let Some(lib) = root.get("lib") {
            let config: dream_abi::library::LibraryConfig =
                lib.clone().try_into().map_err(|e| format!("[lib]: {e}"))?;
            if root
                .get("package")
                .and_then(|p| p.get("type"))
                .and_then(toml::Value::as_str)
                != Some("lib")
            {
                return Err("[lib] requires [package].type = \"lib\"".into());
            }
            m.library = Some(config);
        }
        if let Some(pkg) = root.get("package") {
            let pkg = pkg.as_table().ok_or("[package] must be a table")?;
            m.package_name = opt_string(pkg, "name", "package")?;
            m.entry = opt_string(pkg, "entry", "package")?;
            m.links = opt_string(pkg, "links", "package")?;
        }
        if let Some(gens) = root.get("generators") {
            let gens = gens
                .as_array()
                .ok_or("[[generators]] must be an array of tables")?;
            for g in gens {
                let t = g
                    .as_table()
                    .ok_or("[[generators]] entries must be tables")?;
                if let Some(p) = opt_string(t, "path", "generators")? {
                    if !p.is_empty() {
                        m.generators.push(p);
                    }
                }
            }
        }
        if let Some(native) = root.get("native") {
            let native = native
                .as_table()
                .ok_or("[native] must be a table of sets")?;
            let mut names: Vec<&String> = native.keys().collect();
            names.sort();
            for name in names {
                let set = native[name]
                    .as_table()
                    .ok_or_else(|| format!("[native.{name}] must be a table"))?;
                m.native.push((name.clone(), parse_set(name, set)?));
            }
        }
        Ok(m)
    }
}

const OS_KEYS: [&str; 4] = ["macos", "linux", "windows", "wasm"];
const LIST_KEYS: [&str; 6] = [
    "sources",
    "include",
    "defines",
    "cflags",
    "frameworks",
    "libs",
];

fn parse_set(name: &str, set: &toml::Table) -> Result<NativeSetSpec, String> {
    let mut spec = NativeSetSpec::default();
    for (key, value) in set {
        if OS_KEYS.contains(&key.as_str()) {
            let t = value
                .as_table()
                .ok_or_else(|| format!("[native.{name}.{key}] must be a table"))?;
            if let Some(nested) = t.keys().find(|k| OS_KEYS.contains(&k.as_str())) {
                return Err(format!(
                    "[native.{name}.{key}] cannot nest another OS table ('{nested}')"
                ));
            }
            let parsed = parse_table(&format!("native.{name}.{key}"), t)?;
            match key.as_str() {
                "macos" => spec.macos = parsed,
                "linux" => spec.linux = parsed,
                "windows" => spec.windows = parsed,
                _ => spec.wasm = parsed,
            }
        } else if !LIST_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "unknown key '{key}' in [native.{name}] (expected one of {}, or an OS table: {})",
                LIST_KEYS.join(", "),
                OS_KEYS.join(", ")
            ));
        }
    }
    let base_only: toml::Table = set
        .iter()
        .filter(|(k, _)| !OS_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    spec.base = parse_table(&format!("native.{name}"), &base_only)?;
    Ok(spec)
}

fn parse_table(ctx: &str, t: &toml::Table) -> Result<NativeTable, String> {
    let mut out = NativeTable::default();
    for (key, value) in t {
        let list = string_list(ctx, key, value)?;
        match key.as_str() {
            "sources" => out.sources = list,
            "include" => out.include = list,
            "defines" => out.defines = list,
            "cflags" => out.cflags = list,
            "frameworks" => out.frameworks = list,
            "libs" => out.libs = list,
            other => {
                return Err(format!(
                    "unknown key '{other}' in [{ctx}] (expected one of {})",
                    LIST_KEYS.join(", ")
                ))
            }
        }
    }
    Ok(out)
}

fn string_list(ctx: &str, key: &str, value: &toml::Value) -> Result<Vec<String>, String> {
    let arr = value
        .as_array()
        .ok_or_else(|| format!("[{ctx}].{key} must be an array of strings"))?;
    arr.iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("[{ctx}].{key} must be an array of strings"))
        })
        .collect()
}

fn opt_string(t: &toml::Table, key: &str, table: &str) -> Result<Option<String>, String> {
    match t.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("[{table}].{key} must be a string")),
    }
}

/// Walks from `start` (a file or directory) upward looking for `dream.toml`.
pub fn find_project_root_from(start: &Path) -> Option<PathBuf> {
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

/// Maps a package name to its `import` segment / `dream_packages/` directory name (mirrors
/// `dreamer`'s `import_segment`).
pub fn import_segment(package_name: &str) -> String {
    package_name.replace(['-', '.'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_output_is_one_validated_manifest_string() {
        let prefix = "[package]\nname = \"embed\"\ntype = \"lib\"\n[lib]\n";
        for (value, kind) in [
            ("staticlib", dream_abi::library::LibraryKind::Staticlib),
            ("cdylib", dream_abi::library::LibraryKind::Cdylib),
        ] {
            let manifest =
                ProjectManifest::parse(&format!("{prefix}output-type = \"{value}\"\n")).unwrap();
            assert_eq!(manifest.library.unwrap().output_type, kind);
        }
        for setting in [
            "output-type = []",
            "output-type = [\"staticlib\"]",
            "output-type = \"unknown\"",
            "crate-type = \"staticlib\"",
        ] {
            assert!(ProjectManifest::parse(&format!("{prefix}{setting}\n")).is_err());
        }
        assert!(ProjectManifest::parse("[lib]\noutput-type = \"staticlib\"").is_err());
    }

    #[test]
    fn parses_package_generators_and_native_sets() {
        let m = ProjectManifest::parse(
            r#"
[package]
name = "kv-store"
entry = "src/main.dream"
links = "kv"

[[generators]]
path = "gen/a.dream"

[native.kv_store]
cflags = ["-O2"]
defines = ["A=1"]

[native.kv_store.macos]
frameworks = ["Security"]

[native.kv_store.linux]
libs = ["dl"]

[native.extra]
sources = ["third_party/x.c"]
"#,
        )
        .unwrap();
        assert_eq!(m.package_name.as_deref(), Some("kv-store"));
        assert_eq!(m.entry.as_deref(), Some("src/main.dream"));
        assert_eq!(m.links.as_deref(), Some("kv"));
        assert_eq!(m.generators, vec!["gen/a.dream"]);
        assert_eq!(m.native.len(), 2);
        assert_eq!(m.native[0].0, "extra");
        let kv = &m.native[1].1;
        assert_eq!(kv.base.cflags, vec!["-O2"]);
        assert_eq!(kv.macos.frameworks, vec!["Security"]);
        assert_eq!(kv.linux.libs, vec!["dl"]);
        let host = kv.for_target(&dream_abi::target::TargetSpec::host());
        assert_eq!(host.defines, vec!["A=1"]);
    }

    #[test]
    fn rejects_unknown_native_keys() {
        let err = ProjectManifest::parse("[native.a]\nsource = [\"x.c\"]\n").unwrap_err();
        assert!(err.contains("unknown key 'source'"), "{}", err);
        let err = ProjectManifest::parse("[native.a.freebsd]\nlibs = []\n").unwrap_err();
        assert!(err.contains("unknown key 'freebsd'"), "{}", err);
        let err = ProjectManifest::parse("[native.a]\nlibs = \"dl\"\n").unwrap_err();
        assert!(err.contains("must be an array"), "{}", err);
    }

    #[test]
    fn ignores_package_manager_tables() {
        let m = ProjectManifest::parse(
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[dependencies]\nfoo = \"^1\"\n",
        )
        .unwrap();
        assert_eq!(m.package_name.as_deref(), Some("x"));
        assert!(m.native.is_empty());
    }
}
