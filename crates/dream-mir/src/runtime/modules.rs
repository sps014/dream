//! Core/sys runtime layers and optional linked C libraries (PCRE2 regex).

use std::path::{Path, PathBuf};

use dream_abi::intrinsics::{
    ATTR_REGEX_COMPILE, ATTR_REGEX_FIND, ATTR_REGEX_FIND_ALL, ATTR_REGEX_FREE,
    ATTR_REGEX_GROUP_COUNT, ATTR_REGEX_NAME_AT, ATTR_REGEX_NAME_COUNT, ATTR_REGEX_NAME_NUMBER,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeNeed(u32);

impl RuntimeNeed {
    pub const CORE: Self = Self(1 << 0);
    pub const REGEX: Self = Self(1 << 1);

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Every set a program can need: `CORE` with each combination of the optional modules.
    pub fn all_sets() -> Vec<Self> {
        let mut sets = vec![Self::CORE];
        for m in RUNTIME_MODULES.iter().filter(|m| m.need != Self::CORE) {
            let with: Vec<Self> = sets.iter().map(|s| s.union(m.need)).collect();
            sets.extend(with);
        }
        sets
    }

    pub fn name(self) -> &'static str {
        if self == Self::CORE {
            "core"
        } else if self == Self::REGEX {
            "regex"
        } else {
            "mixed"
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeModule {
    pub id: &'static str,
    pub need: RuntimeNeed,
    pub shared_c: &'static [&'static str],
    pub native_extra_c: &'static [&'static str],
    /// Path relative to `runtime/c` of a vendor file list (`pcre2/SOURCES`).
    pub vendor_sources: Option<&'static str>,
    pub wasm_defines: &'static [&'static str],
    pub native_defines: &'static [&'static str],
    pub include_dirs: &'static [&'static str],
    pub exports: &'static [&'static str],
    pub intrinsic_keys: &'static [&'static str],
}

pub const RUNTIME_MODULES: &[RuntimeModule] = &[RuntimeModule {
    id: "regex",
    need: RuntimeNeed::REGEX,
    shared_c: &["regex.c"],
    native_extra_c: &["pcre2/pcre2_jit_compile.c"],
    vendor_sources: Some("pcre2/SOURCES"),
    wasm_defines: &[
        "HAVE_CONFIG_H",
        "PCRE2_CODE_UNIT_WIDTH=16",
        "PCRE2_STATIC",
        "PCRE2_WASM",
    ],
    native_defines: &[
        "DREAM_NATIVE",
        "HAVE_CONFIG_H",
        "PCRE2_CODE_UNIT_WIDTH=16",
        "PCRE2_STATIC",
    ],
    include_dirs: &["include", "pcre2"],
    exports: &[
        "regex_compile",
        "regex_free",
        "regex_group_count",
        "regex_name_count",
        "regex_name_at",
        "regex_name_number",
        "regex_find",
        "regex_find_all",
        "regex_test",
    ],
    intrinsic_keys: &[
        ATTR_REGEX_COMPILE,
        ATTR_REGEX_FREE,
        ATTR_REGEX_FIND,
        ATTR_REGEX_FIND_ALL,
        ATTR_REGEX_GROUP_COUNT,
        ATTR_REGEX_NAME_COUNT,
        ATTR_REGEX_NAME_AT,
        ATTR_REGEX_NAME_NUMBER,
    ],
}];

/// Portable runtime logic; no OS headers or direct hosted allocator calls.
pub const CORE_C: &[&str] = &[
    "publish.c",
    "region.c",
    "strings.c",
    "string_view.c",
    "ffi.c",
    "object.c",
    "format.c",
    "panic.c",
    "weak.c",
    "closure.c",
    "simd.c",
    "defer.c",
    "platform.c",
    "inlines.c",
    "memory.c",
    "utf8.c",
];
const NATIVE_HEAP_C: &[&str] = &["heap.c", "heap_maps.c"];
const SHARED_SYS_C: &[&str] = &["async.c"];
const NATIVE_SYS_C: &[&str] = &[
    "platform.c",
    "heap_debug.c",
    "leak_report.c",
    "callback.c",
    "sync.c",
    "host_support.c",
    "fs.c",
    "file_handle.c",
    "dirs.c",
    "process.c",
    "env.c",
    "time.c",
    "stdio.c",
    "math.c",
    "worker.c",
];
const WASI_SYS_C: &[&str] = &[
    "callback.c",
    "heap.c",
    "interop_libc.c",
    "heap_memory.c",
    "allocation.c",
    "g0.c",
    "g0.s",
    "sync.c",
    "interns.c",
    "platform.c",
];

pub const SOURCE_RUNTIME_C_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/runtime/c");

pub fn core_runtime_include_dir(root: &Path) -> PathBuf {
    root.to_path_buf().join("core/include")
}

pub fn wasm32_runtime_include_dir(root: &Path) -> PathBuf {
    root.to_path_buf().join("sys/wasi/include")
}

pub fn runtime_abi_include_dir(root: &Path) -> PathBuf {
    root.to_path_buf().join("include")
}

/// Shared core plus the WASI/JS platform services and target heap representation.
pub fn wasm32_runtime_c_files(root: &Path) -> Vec<PathBuf> {
    CORE_C
        .iter()
        .map(|name| root.join("core").join(name))
        .chain(
            WASI_SYS_C
                .iter()
                .map(|name| root.join("sys/wasi").join(name)),
        )
        .chain(
            SHARED_SYS_C
                .iter()
                .map(|name| root.join("sys/shared").join(name)),
        )
        .collect()
}

/// One catalog C unit to compile into the wasm32 guest beyond the always-on core
/// (`shared_c` + vendored `SOURCES`, with the module's wasm defines / include dirs).
pub struct Wasm32LinkedUnit {
    pub path: PathBuf,
    pub defines: Vec<String>,
    pub include_dirs: Vec<PathBuf>,
}

/// Linked-library units for `need` on wasm32 (today: PCRE2 regex).
pub fn wasm32_linked_units(root: &Path, need: RuntimeNeed) -> Vec<Wasm32LinkedUnit> {
    let mut units = Vec::new();
    if !need.contains(RuntimeNeed::REGEX) {
        return units;
    }
    let c = root.to_path_buf();
    for m in RUNTIME_MODULES {
        if !need.contains(m.need) {
            continue;
        }
        let mut dirs: Vec<PathBuf> = vec![c.join("include"), core_runtime_include_dir(root)];
        for rel in m.include_dirs {
            let d = c.join(rel);
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
        let defines: Vec<String> = m.wasm_defines.iter().map(|s| (*s).to_string()).collect();
        for rel in m.shared_c {
            units.push(Wasm32LinkedUnit {
                path: c.join(rel),
                defines: defines.clone(),
                include_dirs: dirs.clone(),
            });
        }
        if let Some(list) = m.vendor_sources {
            let parent = Path::new(list).parent().unwrap_or(Path::new("."));
            for name in vendor_c_names_static(list) {
                units.push(Wasm32LinkedUnit {
                    path: c.join(parent).join(name),
                    defines: defines.clone(),
                    include_dirs: dirs.clone(),
                });
            }
        }
    }
    units
}

fn vendor_c_names_static(list_path: &str) -> Vec<&'static str> {
    match list_path {
        "pcre2/SOURCES" => include_str!("c/pcre2/SOURCES")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn runtime_need_from_keys<'a, I>(keys: I) -> RuntimeNeed
where
    I: IntoIterator<Item = &'a str>,
{
    let mut need = RuntimeNeed::CORE;
    for key in keys {
        for m in RUNTIME_MODULES {
            if m.intrinsic_keys.contains(&key) {
                need.insert(m.need);
            }
        }
    }
    need
}

pub fn runtime_need_from_mir(mir: &crate::Mir) -> RuntimeNeed {
    runtime_need_from_keys(mir.intrinsics.iter().map(|(_, k)| k.as_str()))
}

pub fn runtime_need_from_module_text(src: &str) -> RuntimeNeed {
    let mut need = RuntimeNeed::CORE;
    for m in RUNTIME_MODULES {
        if m.need == RuntimeNeed::CORE {
            continue;
        }
        if m.exports.iter().any(|e| src.contains(e)) {
            need.insert(m.need);
        }
    }
    need
}

pub struct NativeCompileUnit {
    pub path: PathBuf,
    pub defines: Vec<String>,
    pub include_dirs: Vec<PathBuf>,
}

fn catalog_include_dirs(root: &Path, m: &RuntimeModule) -> Vec<PathBuf> {
    let c = root.to_path_buf();
    let mut dirs = vec![core_runtime_include_dir(root), c.join("include")];
    for rel in m.include_dirs {
        let p = c.join(rel);
        if !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    }
    dirs
}

fn push_unit(root: &Path, units: &mut Vec<NativeCompileUnit>, path: PathBuf, m: &RuntimeModule) {
    units.push(NativeCompileUnit {
        path,
        defines: m.native_defines.iter().map(|s| (*s).to_string()).collect(),
        include_dirs: catalog_include_dirs(root, m),
    });
}

/// Native objects for `need`: always-on host C plus catalog `shared_c`/`native_extra_c`/`SOURCES`
/// for live linked modules.
pub fn native_runtime_units(root: &Path, need: RuntimeNeed) -> Vec<NativeCompileUnit> {
    let native_inc = core_runtime_include_dir(root);
    let mut units = Vec::new();
    for (layer, files) in [
        ("core", CORE_C),
        ("core", NATIVE_HEAP_C),
        ("sys/shared", SHARED_SYS_C),
        ("sys/native", NATIVE_SYS_C),
    ] {
        for name in files {
            units.push(NativeCompileUnit {
                path: root.join(layer).join(name),
                defines: vec!["DREAM_NATIVE".into()],
                include_dirs: vec![
                    native_inc.clone(),
                    root.join("sys/native/include"),
                    root.join("include"),
                ],
            });
        }
    }
    let c = root.to_path_buf();
    for m in RUNTIME_MODULES {
        if !need.contains(m.need) {
            continue;
        }
        for rel in m.shared_c {
            push_unit(root, &mut units, c.join(rel), m);
        }
        if let Some(list) = m.vendor_sources {
            let parent = Path::new(list).parent().unwrap_or(Path::new("."));
            for name in vendor_c_names_static(list) {
                push_unit(root, &mut units, c.join(parent).join(name), m);
            }
        }
        for rel in m.native_extra_c {
            push_unit(root, &mut units, c.join(rel), m);
        }
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_sources_exist_on_disk() {
        let c = PathBuf::from(SOURCE_RUNTIME_C_DIR);
        assert!(!RUNTIME_MODULES.is_empty());
        for m in RUNTIME_MODULES {
            for rel in m.shared_c.iter().chain(m.native_extra_c) {
                assert!(c.join(rel).is_file(), "{}", rel);
            }
            if let Some(list) = m.vendor_sources {
                assert!(c.join(list).is_file(), "{}", list);
                let parent = Path::new(list).parent().unwrap_or(Path::new("."));
                let names = vendor_c_names_static(list);
                assert!(!names.is_empty(), "{}", list);
                for name in names {
                    assert!(
                        c.join(parent).join(name).is_file(),
                        "{}/{}",
                        parent.display(),
                        name
                    );
                }
            }
        }
        for unit in native_runtime_units(&c, RuntimeNeed::CORE) {
            assert!(unit.path.is_file(), "{}", unit.path.display());
        }
        for path in wasm32_runtime_c_files(&c) {
            assert!(path.is_file(), "{}", path.display());
        }
    }

    #[test]
    fn regex_need_from_intrinsic_keys() {
        let n = runtime_need_from_keys(["regex_compile", "print"]);
        assert!(n.contains(RuntimeNeed::CORE));
        assert!(n.contains(RuntimeNeed::REGEX));
        let core = runtime_need_from_keys(["print"]);
        assert!(core.contains(RuntimeNeed::CORE));
        assert!(!core.contains(RuntimeNeed::REGEX));
    }

    fn native_runtime_c_files(need: RuntimeNeed) -> Vec<PathBuf> {
        native_runtime_units(Path::new(SOURCE_RUNTIME_C_DIR), need)
            .into_iter()
            .map(|u| u.path)
            .collect()
    }

    #[test]
    fn native_host_units_are_included_once() {
        let core = native_runtime_c_files(RuntimeNeed::CORE);
        for name in [
            "host_support.c",
            "fs.c",
            "file_handle.c",
            "dirs.c",
            "process.c",
            "env.c",
            "time.c",
            "stdio.c",
            "math.c",
        ] {
            assert_eq!(
                core.iter()
                    .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some(name))
                    .count(),
                1,
                "{name}"
            );
        }
        assert!(core
            .iter()
            .all(|p| p.file_name().and_then(|n| n.to_str()) != Some("host.c")));
    }

    #[test]
    fn native_units_tree_shake_pcre2() {
        let core = native_runtime_c_files(RuntimeNeed::CORE);
        assert!(core.iter().all(|p| !p.to_string_lossy().contains("pcre2")));
        assert!(core
            .iter()
            .all(|p| p.file_name().and_then(|n| n.to_str()) != Some("regex.c")));
        let with = native_runtime_c_files(RuntimeNeed::CORE.union(RuntimeNeed::REGEX));
        assert!(with
            .iter()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("regex.c")));
        assert!(with
            .iter()
            .any(|p| p.to_string_lossy().contains("pcre2_compile.c")));
        assert!(with
            .iter()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("pcre2_jit_compile.c")));
    }
}
