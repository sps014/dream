use super::tools::LlvmTools;
use crate::driver::rt_stamp;
use crate::driver::wasi::{GuestUnitOutput, guest_include_dirs, unit_command};
use crate::driver::wasm_opt::OptLevel;
use dream_abi::target::TargetSpec;
use dream_mir::runtime::RuntimeNeed;
use rayon::prelude::*;
use std::path::Path;
use std::path::PathBuf;

fn compile_objects<T: Sync>(
    units: &[T],
    compile: impl Fn(usize, &T) -> Result<PathBuf, String> + Sync,
) -> Result<Vec<PathBuf>, String> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|error| format!("runtime compilation pool: {error}"))?;
    // Indexed collection preserves link order and selects errors in source order.
    pool.install(|| {
        units
            .par_iter()
            .enumerate()
            .map(|(index, unit)| compile(index, unit))
            .collect::<Vec<_>>()
    })
    .into_iter()
    .collect()
}

pub(super) fn store_prebuilt(dir: &Path, objects: &[PathBuf]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut manifest = Vec::new();
    for object in objects {
        let name = object
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid runtime object filename")?;
        std::fs::copy(object, dir.join(name)).map_err(|e| e.to_string())?;
        let hash = rt_stamp::content_hash(object).ok_or("unreadable runtime object")?;
        manifest.push((name.to_string(), hash.to_hex().to_string()));
    }
    std::fs::write(
        dir.join("objects.json"),
        serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn load_prebuilt(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let manifest: Vec<(String, String)> = serde_json::from_slice(
        &std::fs::read(dir.join("objects.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if manifest.is_empty() {
        return Err("prebuilt runtime has no objects".into());
    }
    manifest
        .into_iter()
        .map(|(name, hash)| {
            if Path::new(&name).components().count() != 1
                || name.contains(['/', '\\', ':'])
                || matches!(name.as_str(), "." | "..")
            {
                return Err("invalid prebuilt runtime object path".into());
            }
            let path = dir.join(name);
            if rt_stamp::content_hash(&path).is_none_or(|digest| digest.to_hex().as_str() != hash) {
                return Err(format!(
                    "corrupt prebuilt runtime object {}",
                    path.display()
                ));
            }
            Ok(path)
        })
        .collect()
}

pub(super) fn wasm(
    tools: &LlvmTools,
    need: RuntimeNeed,
    threads: bool,
) -> Result<Vec<PathBuf>, String> {
    let config = &tools.config;
    if let super::bundle::RtDir::Prebuilt(dir) =
        super::bundle::rt_dir(config, super::wasm::flavor(threads), OptLevel::O0, need)
    {
        return load_prebuilt(&dir.join("unit-objects"));
    }
    let clang = tools.clang()?;
    let sysroot = super::wasm::wasi_sysroot(&clang)?;
    let identity = format!(
        "wasm-objects-v1:{need:?}:{threads}:{}:{}:{:?}:{:?}",
        config.fingerprint(),
        rt_stamp::content_fingerprint(vec![clang.clone()]),
        crate::driver::wasi::GUEST_FEATURES,
        dream_mir::runtime::WASM32_LIBC_UNITS
    );
    let dir = config
        .native_rt_cache_root()
        .join("wasm-objects")
        .join(blake3::hash(identity.as_bytes()).to_hex().as_str());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let include_dirs = guest_include_dirs(&config.runtime_c);
    let includes: Vec<_> = include_dirs.iter().map(PathBuf::as_path).collect();
    let namespaces = Default::default();
    let units = super::wasm::units(&config.runtime_c, need);
    compile_objects(&units, |index, unit| {
        let output = dir.join(format!("runtime-{index}.o"));
        let mut command = unit_command(
            &clang,
            &sysroot,
            &unit.path,
            &includes,
            &unit.include_dirs,
            &unit.defines,
            threads,
            OptLevel::O0,
            GuestUnitOutput {
                stable_name: &format!("rt{index}.c"),
                bitcode: false,
            },
        );
        if unit.path.extension().is_some_and(|ext| ext == "s") {
            command.args(["-x", "assembler-with-cpp"]);
        }
        if config.runtime_counters {
            command.arg("-DDREAM_RUNTIME_COUNTERS=1");
        }
        command.arg(&unit.path);
        let unit = super::runtime_cache::compile(
            command,
            &config.native_rt_cache_root(),
            &output,
            config.compiler_environment.is_empty(),
            &namespaces,
        )?;
        Ok(unit.object)
    })
}

pub(super) fn native(
    tools: &LlvmTools,
    spec: &TargetSpec,
    need: RuntimeNeed,
    debug_info: bool,
) -> Result<Vec<PathBuf>, String> {
    let config = &tools.config;
    if spec.can_link_on_host()
        && super::runtime::native_sanitize_flag(config, spec).is_none()
        && let super::bundle::RtDir::Prebuilt(dir) =
            super::bundle::rt_dir(config, "native", OptLevel::O0, need)
    {
        return load_prebuilt(&dir.join("unit-objects"));
    }
    let clang = tools.clang()?;
    let identity = format!(
        "native-objects-v1:{spec:?}:{need:?}:{debug_info}:{}:{}",
        config.fingerprint(),
        rt_stamp::content_fingerprint(vec![clang.clone()])
    );
    let dir = config
        .native_rt_cache_root()
        .join("native-objects")
        .join(blake3::hash(identity.as_bytes()).to_hex().as_str());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (mut units, vendors) = super::runtime::bitcode_units(&config.runtime_c, need);
    units.extend(vendors);
    let mut flags = vec!["-O0"];
    if !spec.is_windows() {
        flags.push("-fPIC");
    }
    if debug_info {
        flags.push("-g");
    }
    let namespaces = Default::default();
    compile_objects(&units, |index, unit| {
        let output = dir.join(format!("runtime-{index}.o"));
        let unit =
            super::runtime::clang_unit(config, spec, &clang, unit, &flags, &output, &namespaces)?;
        Ok(unit.object)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebuilt_objects_require_valid_contents_and_contained_paths() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("runtime.o");
        std::fs::write(&source, b"runtime object").unwrap();
        let bundle = temp.path().join("bundle");
        store_prebuilt(&bundle, &[source]).unwrap();
        let objects = load_prebuilt(&bundle).unwrap();
        assert_eq!(objects.len(), 1);
        std::fs::write(&objects[0], b"corrupt object").unwrap();
        assert!(load_prebuilt(&bundle).is_err());
        for name in ["../runtime.o", "/runtime.o", "..", "dir\\runtime.o"] {
            std::fs::write(
                bundle.join("objects.json"),
                serde_json::to_vec(&[(name, "unused")]).unwrap(),
            )
            .unwrap();
            assert!(load_prebuilt(&bundle).is_err());
        }
        std::fs::write(bundle.join("objects.json"), b"[]").unwrap();
        assert!(load_prebuilt(&bundle).is_err());
        std::fs::write(bundle.join("objects.json"), b"partial").unwrap();
        assert!(load_prebuilt(&bundle).is_err());
    }
}
