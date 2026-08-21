//! Linked C libraries (today: PCRE2 regex) plus native compile file lists.
//! Native C uses `runtime/c/native/`; the wasm32 guest uses `runtime/c/wasm32/` + shared `native/`
//! units.

use std::path::{Path, PathBuf};

use dream_abi::intrinsics::{
    ATTR_REGEX_COMPILE, ATTR_REGEX_FIND, ATTR_REGEX_FREE, ATTR_REGEX_GROUP_COUNT,
    ATTR_REGEX_NAME_AT, ATTR_REGEX_NAME_COUNT, ATTR_REGEX_NAME_NUMBER,
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
        "regex_test",
    ],
    intrinsic_keys: &[
        ATTR_REGEX_COMPILE,
        ATTR_REGEX_FREE,
        ATTR_REGEX_FIND,
        ATTR_REGEX_GROUP_COUNT,
        ATTR_REGEX_NAME_COUNT,
        ATTR_REGEX_NAME_AT,
        ATTR_REGEX_NAME_NUMBER,
    ],
}];

const NATIVE_CORE_C: &[&str] = &[
    "heap.c",
    "strings.c",
    "ffi.c",
    "object.c",
    "format.c",
    "panic.c",
    "weak.c",
    "closure.c",
    "async.c",
    "sync.c",
    "simd.c",
    "host.c",
    "worker.c",
    "defer.c",
];

pub fn runtime_c_dir() -> PathBuf {
    if let Ok(p) = std::env::var("DREAM_RUNTIME_C") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime/c");
    if crate_dir.join("native/include/dream_rt_native.h").is_file() {
        return crate_dir;
    }
    for candidate in installed_runtime_c_dirs() {
        if candidate.join("native/include/dream_rt_native.h").is_file() {
            return candidate;
        }
    }
    crate_dir
}

fn installed_runtime_c_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = std::env::var("DREAM_HOME") {
        let p = PathBuf::from(home);
        out.push(p.join("lib/runtime/c"));
        if p.file_name().and_then(|s| s.to_str()) == Some("bin") {
            if let Some(parent) = p.parent() {
                out.push(parent.join("lib/runtime/c"));
            }
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        out.push(PathBuf::from(home).join(".dream/lib/runtime/c"));
    }
    out
}

pub fn native_runtime_include_dir() -> PathBuf {
    runtime_c_dir().join("native/include")
}

pub fn wasm32_runtime_include_dir() -> PathBuf {
    runtime_c_dir().join("wasm32/include")
}

pub fn wasm32_heap_c() -> PathBuf {
    runtime_c_dir().join("wasm32/heap.c")
}

pub fn runtime_abi_include_dir() -> PathBuf {
    runtime_c_dir().join("include")
}

pub fn wasm32_libc_c() -> PathBuf {
    runtime_c_dir().join("wasm32/libc.c")
}

const WASM32_CORE_C: &[&str] = &[
    "wasm32/heap.c",
    "wasm32/libc.c",
    "wasm32/g0.c",
    "wasm32/g0.s",
    "wasm32/sync_stub.c",
    "wasm32/weak_stub.c",
    "wasm32/interns.c",
    "native/strings.c",
    "native/object.c",
    "native/format.c",
    "native/panic.c",
    "native/closure.c",
    "native/async.c",
    "native/defer.c",
    "native/simd.c",
    "native/ffi.c",
];

/// Guest runtime C units for MIR → C → wasm32 (wasi-sdk). Skips native mmap heap, libc host, and pthreads.
pub fn wasm32_runtime_c_files() -> Vec<PathBuf> {
    let c = runtime_c_dir();
    WASM32_CORE_C.iter().map(|rel| c.join(rel)).collect()
}

/// One catalog C unit to compile into the wasm32 guest beyond the always-on core
/// (`shared_c` + vendored `SOURCES`, with the module's wasm defines / include dirs).
pub struct Wasm32LinkedUnit {
    pub path: PathBuf,
    pub defines: Vec<String>,
    pub include_dirs: Vec<PathBuf>,
}

/// Linked-library units for `need` on wasm32 (today: PCRE2 regex).
pub fn wasm32_linked_units(need: RuntimeNeed) -> Vec<Wasm32LinkedUnit> {
    let mut units = Vec::new();
    if !need.contains(RuntimeNeed::REGEX) {
        return units;
    }
    let c = runtime_c_dir();
    for m in RUNTIME_MODULES {
        if !need.contains(m.need) {
            continue;
        }
        let mut dirs: Vec<PathBuf> =
            vec![c.join("include"), native_runtime_include_dir()];
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

pub fn native_pcre2_include_dir() -> PathBuf {
    runtime_c_dir().join("pcre2")
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

pub fn runtime_need_from_c_source(src: &str) -> RuntimeNeed {
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

fn catalog_include_dirs(m: &RuntimeModule) -> Vec<PathBuf> {
    let c = runtime_c_dir();
    let mut dirs = vec![native_runtime_include_dir(), c.join("include")];
    for rel in m.include_dirs {
        let p = c.join(rel);
        if !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    }
    dirs
}

fn push_unit(units: &mut Vec<NativeCompileUnit>, path: PathBuf, m: &RuntimeModule) {
    units.push(NativeCompileUnit {
        path,
        defines: m.native_defines.iter().map(|s| (*s).to_string()).collect(),
        include_dirs: catalog_include_dirs(m),
    });
}

/// Native objects for `need`: always-on host C plus catalog `shared_c`/`native_extra_c`/`SOURCES`
/// for live linked modules.
pub fn native_runtime_units(need: RuntimeNeed) -> Vec<NativeCompileUnit> {
    let native = runtime_c_dir().join("native");
    let native_inc = native_runtime_include_dir();
    let mut units = Vec::new();
    for name in NATIVE_CORE_C {
        units.push(NativeCompileUnit {
            path: native.join(name),
            defines: vec!["DREAM_NATIVE".into()],
            include_dirs: vec![native_inc.clone()],
        });
    }
    let c = runtime_c_dir();
    for m in RUNTIME_MODULES {
        if !need.contains(m.need) {
            continue;
        }
        for rel in m.shared_c {
            push_unit(&mut units, c.join(rel), m);
        }
        if let Some(list) = m.vendor_sources {
            let parent = Path::new(list).parent().unwrap_or(Path::new("."));
            for name in vendor_c_names_static(list) {
                push_unit(&mut units, c.join(parent).join(name), m);
            }
        }
        for rel in m.native_extra_c {
            push_unit(&mut units, c.join(rel), m);
        }
    }
    units
}

pub fn native_runtime_c_files(need: RuntimeNeed) -> Vec<PathBuf> {
    native_runtime_units(need)
        .into_iter()
        .map(|u| u.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_sources_exist_on_disk() {
        let c = runtime_c_dir();
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
        for name in NATIVE_CORE_C {
            assert!(c.join("native").join(name).is_file(), "native/{}", name);
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
