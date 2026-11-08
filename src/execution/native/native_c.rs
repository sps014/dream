//! Compiles a program's live `native/` C/C++ source sets (the `.abi.json` `c_sources` list) to
//! objects with the pinned clang that also links the binary, so C++ objects and the final link
//! agree on one C++ standard library. Each object keeps a stamp of the compiler, the command, and
//! the headers clang recorded, and is rebuilt only when one of those changes.

use super::driver::{Lang, compiler};
use crate::driver::rt_stamp;
use crate::driver::wasi::run_captured;
use dream_mir::runtime::runtime_abi_include_dir;
use serde::Deserialize;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CSourceSet {
    pub name: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub libs: Vec<String>,
    /// Runtime functions the set's code calls (generated `@cpp` shims), kept exported by `opt`.
    #[serde(default)]
    pub runtime_exports: Vec<String>,
    /// Release builds link this set's bitcode. `false` keeps the object files.
    #[serde(default = "default_lto")]
    pub lto: bool,
}

fn default_lto() -> bool {
    true
}

#[derive(Deserialize)]
struct AbiFile {
    #[serde(default)]
    c_sources: Vec<CSourceSet>,
}

pub fn read_c_sources_from_abi(abi_path: &Path) -> Result<Vec<CSourceSet>, String> {
    let text = match std::fs::read_to_string(abi_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!("cannot read {}: {error}", abi_path.display()));
        }
    };
    serde_json::from_str::<AbiFile>(&text)
        .map(|abi| abi.c_sources)
        .map_err(|error| format!("{}: invalid ABI JSON: {error}", abi_path.display()))
}

/// Everything the link step needs from the compiled sets.
#[derive(Debug, Default)]
pub struct NativeObjects {
    pub objects: Vec<PathBuf>,
    /// Bitcode of sets that a release build links before optimization.
    pub lto_bitcode: Vec<PathBuf>,
    /// LLVM bitcode of each source, used to check Dream's declared C types.
    pub bitcode: Vec<PathBuf>,
    pub link_args: Vec<String>,
    /// Links in C++ mode, which adds the target's C++ standard library.
    pub needs_cxx: bool,
    pub runtime_exports: Vec<String>,
}

fn is_cxx(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("cpp" | "cc" | "cxx" | "mm")
    )
}

/// Compiles every set to objects under `cache_root/<set>/`.
pub fn compile_sets(
    config: &crate::driver::toolchain::ToolchainConfig,
    clang: &Path,
    spec: &dream_abi::target::TargetSpec,
    sets: &[CSourceSet],
    cache_root: &Path,
    debug: bool,
    objects: bool,
) -> Result<NativeObjects, String> {
    let mut out = NativeObjects::default();
    let embed_include = runtime_abi_include_dir(&config.runtime_c);
    for set in sets {
        let dir = cache_root.join(&set.name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(".lock"))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        lock.lock()
            .map_err(|e| format!("locking {}: {e}", dir.display()))?;
        for src in &set.sources {
            let src = PathBuf::from(src);
            let obj = dir.join(object_name(&src));
            let bitcode = obj.with_extension("bc");
            let cxx = is_cxx(&src);
            out.needs_cxx |= cxx;
            let args = compile_args(set, &embed_include, &src, &bitcode, cxx, debug, spec);
            let lang = if cxx { Lang::Cxx } else { Lang::C };
            let mut command = compiler(clang, config, spec, lang);
            command.args(&args);
            compile_if_stale(
                config,
                clang,
                spec,
                &mut command,
                &bitcode,
                &format!("compiling {}", src.display()),
            )?;
            out.bitcode.push(bitcode.clone());
            if objects || !set.lto {
                object_from_bitcode(config, clang, spec, &bitcode, &obj)?;
                out.objects.push(obj);
            } else {
                out.lto_bitcode.push(bitcode);
            }
        }
        if !set.frameworks.is_empty() && !spec.is_apple() {
            eprintln!(
                "warning: C/C++ set '{}' lists frameworks, which are only linked on Apple targets",
                set.name
            );
        }
        for fw in &set.frameworks {
            if spec.is_apple() {
                out.link_args.push("-framework".into());
                out.link_args.push(fw.clone());
            }
        }
        for lib in &set.libs {
            out.link_args.push(format!("-l{lib}"));
        }
        for e in &set.runtime_exports {
            if !out.runtime_exports.contains(e) {
                out.runtime_exports.push(e.clone());
            }
        }
    }
    Ok(out)
}

/// `embed_include` holds the public `dream_embed.h`, after the set's own include dirs.
fn object_from_bitcode(
    config: &crate::driver::toolchain::ToolchainConfig,
    clang: &Path,
    spec: &dream_abi::target::TargetSpec,
    bitcode: &Path,
    object: &Path,
) -> Result<(), String> {
    let current = std::fs::metadata(object)
        .and_then(|meta| meta.modified())
        .ok();
    let source = std::fs::metadata(bitcode)
        .and_then(|meta| meta.modified())
        .ok();
    if matches!((current, source), (Some(object_time), Some(bitcode_time)) if object_time >= bitcode_time)
    {
        return Ok(());
    }
    let mut command = compiler(clang, config, spec, Lang::C);
    command.arg("-c");
    // Bitcode has no section assignment. ELF `--gc-sections` can drop an unused function
    // only when this codegen step puts each function in its own section.
    if !spec.is_windows() && !spec.is_apple() {
        command.args(["-ffunction-sections", "-fdata-sections"]);
    }
    command.arg(bitcode).arg("-o").arg(object);
    run_captured(&mut command, &format!("clang ({})", bitcode.display()))
}

fn compile_args(
    set: &CSourceSet,
    embed_include: &Path,
    src: &Path,
    obj: &Path,
    cxx: bool,
    debug: bool,
    spec: &dream_abi::target::TargetSpec,
) -> Vec<String> {
    let mut args = vec![
        "-c".to_string(),
        "-emit-llvm".to_string(),
        if cxx { "-std=gnu++20" } else { "-std=gnu11" }.to_string(),
    ];
    if !spec.is_windows() {
        args.push("-fPIC".to_string());
    }
    if debug {
        args.extend(["-O0".to_string(), "-g".to_string()]);
    } else {
        args.push("-O2".to_string());
    }
    for inc in &set.include {
        args.push(format!("-I{inc}"));
    }
    args.push(format!("-I{}", embed_include.display()));
    for d in &set.defines {
        args.push(format!("-D{d}"));
    }
    args.push("-fno-ident".to_string());
    if let Some(dir) = src.parent() {
        args.push(format!("-ffile-prefix-map={}/{}", dir.display(), "="));
    }
    for inc in &set.include {
        args.push(format!("-ffile-prefix-map={inc}/="));
    }
    args.extend(set.cflags.iter().cloned());
    match src.extension().and_then(|ext| ext.to_str()) {
        Some("m") => args.extend(["-x".to_string(), "objective-c".to_string()]),
        Some("mm") => args.extend(["-x".to_string(), "objective-c++".to_string()]),
        _ => {}
    }
    args.push(src.display().to_string());
    args.push("-o".to_string());
    args.push(obj.display().to_string());
    args
}

/// Runs `command` only when `output` is missing or its stamp no longer matches the compiler,
/// the command line, the target, or a file from the previous dependency list.
pub(crate) fn compile_if_stale(
    config: &crate::driver::toolchain::ToolchainConfig,
    clang: &Path,
    spec: &dream_abi::target::TargetSpec,
    command: &mut Command,
    output: &Path,
    label: &str,
) -> Result<(), String> {
    let deps = sibling(output, "d");
    let stamp = sibling(output, "stamp");
    command
        .args(["-MD", "-MF"])
        .arg(&deps)
        .args(["-MT", "dream-unit"]);
    let key = unit_key(config, clang, spec, command)?;
    let lock_path = sibling(output, "lock");
    if let Some(dir) = lock_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("{}: {e}", lock_path.display()))?;
    lock.lock()
        .map_err(|e| format!("locking {}: {e}", lock_path.display()))?;
    if unit_is_fresh(output, &stamp, &deps, &key) {
        return Ok(());
    }
    if let Err(error) = run_captured(command, label) {
        let _ = std::fs::remove_file(output);
        let _ = std::fs::remove_file(&stamp);
        return Err(error);
    }
    let dependencies = dependency_paths(&deps)?;
    let fingerprint = rt_stamp::content_fingerprint(dependencies);
    let body = format!("native-unit-v1\n{key}\n{fingerprint}");
    let partial = sibling(output, "stamp.partial");
    std::fs::write(&partial, body).map_err(|e| format!("{}: {e}", partial.display()))?;
    std::fs::rename(&partial, &stamp)
        .map_err(|e| format!("renaming {} to {}: {e}", partial.display(), stamp.display()))?;
    Ok(())
}

fn unit_key(
    config: &crate::driver::toolchain::ToolchainConfig,
    clang: &Path,
    spec: &dream_abi::target::TargetSpec,
    command: &Command,
) -> Result<String, String> {
    let identity = rt_stamp::persistent_tool_identity(clang, &config.tool_identity_cache_root())
        .ok_or_else(|| format!("clang identity is unreadable: {}", clang.display()))?;
    Ok(blake3::hash(
        format!("native-unit-v1\n{identity}\n{:?}\n{command:?}", spec.triple).as_bytes(),
    )
    .to_hex()
    .to_string())
}

fn unit_is_fresh(output: &Path, stamp: &Path, deps: &Path, key: &str) -> bool {
    if !output.is_file() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(stamp) else {
        return false;
    };
    let mut lines = text.splitn(3, '\n');
    if lines.next() != Some("native-unit-v1") || lines.next() != Some(key) {
        return false;
    }
    let Some(recorded) = lines.next() else {
        return false;
    };
    let Ok(dependencies) = dependency_paths(deps) else {
        return false;
    };
    if dependencies.is_empty() {
        return false;
    }
    rt_stamp::content_fingerprint(dependencies) == recorded
}

fn dependency_paths(deps: &Path) -> Result<Vec<PathBuf>, String> {
    let text = std::fs::read_to_string(deps).map_err(|e| format!("{}: {e}", deps.display()))?;
    let text = normalize_dependency_separators(&text);
    let parsed = depfile::parse(&text).map_err(|e| format!("{}: {e:?}", deps.display()))?;
    let paths = parsed
        .find("dream-unit")
        .ok_or_else(|| format!("{}: clang omitted the dependency target", deps.display()))?;
    Ok(paths
        .iter()
        .map(|path| PathBuf::from(path.as_ref()))
        .collect())
}

fn sibling(output: &Path, ext: &str) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    output.with_file_name(name)
}

fn normalize_dependency_separators(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut normalized = String::with_capacity(text.len());
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('\\' | ' ' | '\t' | '\r' | '\n' | ':' | '#' | '$') => {
                    normalized.push(ch);
                    normalized.push(chars.next().unwrap());
                    continue;
                }
                Some(_) => {
                    normalized.push('/');
                    continue;
                }
                None => {}
            }
        }
        normalized.push(ch);
    }
    normalized
}

/// A stable, collision-free object name: the source stem plus a hash of its full path.
fn object_name(src: &Path) -> String {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("src");
    format!(
        "{stem}-{:016x}.o",
        fnv1a(src.display().to_string().as_bytes())
    )
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_names_are_stable_and_distinct() {
        let a = object_name(Path::new("/p/native/a/x.c"));
        let b = object_name(Path::new("/p/native/b/x.c"));
        assert_ne!(a, b);
        assert_eq!(a, object_name(Path::new("/p/native/a/x.c")));
        assert!(a.starts_with("x-") && a.ends_with(".o"));
    }

    #[test]
    fn cxx_units_use_gnu20_and_c_units_gnu11() {
        let set = CSourceSet {
            name: "s".into(),
            sources: vec![],
            include: vec!["/inc".into()],
            defines: vec!["A=1".into()],
            cflags: vec!["-Wall".into()],
            frameworks: vec![],
            libs: vec![],
            runtime_exports: vec![],
            lto: true,
        };
        let c = compile_args(
            &set,
            Path::new("/rt"),
            Path::new("a.c"),
            Path::new("a.o"),
            false,
            false,
            &dream_abi::target::TargetSpec::host(),
        );
        assert!(c.contains(&"-emit-llvm".to_string()));
        assert!(c.contains(&"-std=gnu11".to_string()));
        assert_eq!(c.contains(&"-fPIC".to_string()), !cfg!(windows));
        assert!(c.contains(&"-I/inc".to_string()) && c.contains(&"-DA=1".to_string()));
        let cxx = compile_args(
            &set,
            Path::new("/rt"),
            Path::new("a.cpp"),
            Path::new("a.o"),
            true,
            true,
            &dream_abi::target::TargetSpec::host(),
        );
        assert!(cxx.contains(&"-std=gnu++20".to_string()) && cxx.contains(&"-g".to_string()));
    }

    #[test]
    fn a_unit_is_fresh_until_a_header_or_the_command_changes() {
        let dir = std::env::temp_dir().join(format!("dream-native-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("a.c");
        let header = dir.join("a.h");
        std::fs::write(&source, "int a;\n").unwrap();
        std::fs::write(&header, "#define A 1\n").unwrap();
        let output = dir.join("a.o");
        std::fs::write(&output, "object").unwrap();
        let deps = sibling(&output, "d");
        std::fs::write(
            &deps,
            format!("dream-unit: {} {}\n", source.display(), header.display()),
        )
        .unwrap();
        let key = "abc";
        let fingerprint =
            rt_stamp::content_fingerprint(dependency_paths(&deps).expect("dependency paths"));
        std::fs::write(
            sibling(&output, "stamp"),
            format!("native-unit-v1\n{key}\n{fingerprint}"),
        )
        .unwrap();
        assert!(unit_is_fresh(
            &output,
            &sibling(&output, "stamp"),
            &deps,
            key
        ));
        assert!(!unit_is_fresh(
            &output,
            &sibling(&output, "stamp"),
            &deps,
            "other"
        ));
        std::fs::write(&header, "#define A 2\n").unwrap();
        assert!(!unit_is_fresh(
            &output,
            &sibling(&output, "stamp"),
            &deps,
            key
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_dependency_paths_keep_spaces() {
        let text = "dream-unit: D:\\a\\dream\\core.c \\\r\n C:\\Program\\ Files\\sdk.h\r\n";
        let normalized = normalize_dependency_separators(text);
        let parsed = depfile::parse(&normalized).unwrap();
        let paths: Vec<_> = parsed
            .find("dream-unit")
            .unwrap()
            .iter()
            .map(|s| s.as_ref())
            .collect();
        assert_eq!(paths, ["D:/a/dream/core.c", "C:/Program Files/sdk.h"]);
    }
}
