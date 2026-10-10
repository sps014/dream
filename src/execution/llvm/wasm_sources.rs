//! Package C/C++ bodies join the guest before optimization; target libraries remain archives.

use super::tools::LlvmTools;
use crate::driver::wasi::run_captured;
use crate::driver::wasm_opt::OptLevel;
use crate::execution::native::native_c::read_c_sources_from_abi;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct PackageSources {
    pub bitcode: Vec<PathBuf>,
    pub libraries: Vec<PathBuf>,
    pub public: Vec<String>,
    pub exceptions: bool,
}

pub(super) fn compile(
    tools: &LlvmTools,
    ll: &Path,
    threads: bool,
    opt: OptLevel,
) -> Result<PackageSources, String> {
    let sets = read_c_sources_from_abi(&ll.with_extension("abi.json"));
    let libraries =
        crate::execution::native::c_link::read_c_libs_from_abi(&ll.with_extension("abi.json"));
    let mut out = PackageSources::default();
    if sets.is_empty()
        && !libraries
            .iter()
            .any(|lib| crate::driver::native_sets::WASM_C_LIBRARIES.contains(&lib.as_str()))
    {
        return Ok(out);
    }
    let clang = tools.clang()?;
    let sysroot = match super::bundle::prebuilt_rt(&tools.config) {
        Some(root) => root.join("wasi-sysroot"),
        None => super::wasm::wasi_sysroot(&clang)?,
    };
    // Clang resolves nested system headers incorrectly with Windows verbatim paths.
    let sysroot = dunce::simplified(&sysroot);
    let target = if threads {
        "wasm32-wasip1-threads"
    } else {
        "wasm32-wasip1"
    };
    let lib = sysroot.join("lib").join(target);
    let root = ll.with_extension("wasm-c");
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    for set in sets {
        if !set.frameworks.is_empty() {
            return Err(format!(
                "C/C++ set '{}' uses frameworks unavailable on wasm32",
                set.name
            ));
        }
        out.public.extend(set.runtime_exports);
        for (i, src) in set.sources.iter().enumerate() {
            let cxx = crate::driver::native_sets::is_cxx_source(Path::new(src));
            out.exceptions |= cxx;
            let bc = root.join(format!("{}-{i}.bc", set.name));
            let mut cmd = std::process::Command::new(&clang);
            cmd.arg(format!("--target={target}"))
                .arg(format!("--sysroot={}", sysroot.display()))
                .args([
                    "-c",
                    "-emit-llvm",
                    "-g0",
                    "-fno-ident",
                    "-frandom-seed=0",
                    "-mbulk-memory",
                    "-mmutable-globals",
                ])
                .arg(opt.wasm_clang_opt_flag())
                .arg(if cxx { "-std=gnu++20" } else { "-std=gnu11" })
                .arg(format!("-ffile-prefix-map={src}=package/{}/{i}", set.name));
            if cxx {
                let headers = sysroot.join("include").join(target).join("eh/c++/v1");
                if !headers.join("string").is_file() {
                    return Err(format!(
                        "WASM C++ headers not found at {}; run scripts/fetch-dev-llvm.sh (Windows: scripts/fetch-dev-llvm.ps1)",
                        headers.display()
                    ));
                }
                cmd.args(["-nostdinc++", "-isystem"]).arg(headers).args([
                    "-fuse-cxa-atexit",
                    "-fwasm-exceptions",
                    "-mllvm",
                    "-wasm-use-legacy-eh=false",
                ]);
            }
            if threads {
                cmd.arg("-pthread");
            }
            for inc in &set.include {
                cmd.arg("-I").arg(inc);
            }
            cmd.arg("-I")
                .arg(dream_mir::runtime::runtime_abi_include_dir(
                    &tools.config.runtime_c,
                ));
            for define in &set.defines {
                cmd.arg(format!("-D{define}"));
            }
            cmd.args(&set.cflags).arg(src).arg("-o").arg(&bc);
            run_captured(&mut cmd, &format!("WASM C/C++ set '{}' ({src})", set.name))?;
            out.bitcode.push(bc);
        }
        for name in set.libs {
            out.libraries.push(required_library(&lib, &name)?);
        }
    }
    if out.exceptions {
        for name in ["c++", "c++abi", "unwind"] {
            out.libraries.push(required_library(&lib.join("eh"), name)?);
        }
    }
    out.libraries.push(required_library(&lib, "c")?);
    // LLVM's libc definitions must survive even when only archive objects reference them.
    out.public.extend(
        [
            "malloc",
            "calloc",
            "realloc",
            "free",
            "strlen",
            "aligned_alloc",
            "posix_memalign",
            "__libc_malloc",
            "__libc_calloc",
            "__libc_free",
            "malloc_usable_size",
        ]
        .map(str::to_string),
    );
    for name in [
        "environ_get",
        "environ_sizes_get",
        "fd_write",
        "fd_fdstat_get",
        "fd_close",
        "fd_seek",
        "proc_exit",
        "clock_time_get",
    ] {
        out.public
            .push(format!("__imported_wasi_snapshot_preview1_{name}"));
    }
    Ok(out)
}

fn required_library(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(format!("lib{name}.a"));
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "WASM library '{name}' not found at {}; provide a WASI sysroot with this library beside the pinned LLVM",
            path.display()
        ))
    }
}

pub(super) fn validate_signatures(
    tools: &LlvmTools,
    modules: &[PathBuf],
    shim: &Path,
    runtime: &dream_mir::backend::llvm::RuntimeSigs,
) -> Result<(), String> {
    use dream_mir::backend::llvm::RuntimeSigs;
    let expected = RuntimeSigs::parse(&super::runtime::disassemble(tools, shim)?)?;
    let compare = |actual: &RuntimeSigs| -> Result<(), String> {
        for (name, declared) in &expected.fns {
            if let Some(defined) = actual.fns.get(name)
                && declared.fty != defined.fty
            {
                return Err(format!(
                    "WASM C ABI mismatch for '{name}': Dream declares {}, package source uses {}; use usize/isize for pointer-sized C integers",
                    declared.fty, defined.fty
                ));
            }
        }
        Ok(())
    };
    compare(runtime)?;
    for module in modules {
        let actual = RuntimeSigs::parse(&super::runtime::disassemble(tools, module)?)?;
        compare(&actual)?;
    }
    Ok(())
}

pub(super) fn validate_imports(
    ll: &Path,
    wasm: &Path,
    runtime: &dream_mir::backend::llvm::RuntimeSigs,
) -> Result<(), String> {
    let mut allowed = std::collections::BTreeSet::new();
    for function in runtime.fns.values() {
        if let Some(import) = &function.wasm_import {
            allowed.insert(import.clone());
        }
    }
    let abi = std::fs::read_to_string(ll.with_extension("abi.json")).map_err(|e| e.to_string())?;
    let abi: serde_json::Value = serde_json::from_str(&abi).map_err(|e| e.to_string())?;
    if let Some(externs) = abi["externs"].as_array() {
        for entry in externs {
            if let (Some(module), Some(field)) = (entry["module"].as_str(), entry["field"].as_str())
                && !module.starts_with("c/")
            {
                allowed.insert((module.to_string(), field.to_string()));
            }
        }
    }
    let bytes = std::fs::read(wasm).map_err(|e| e.to_string())?;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ImportSection(imports) = payload.map_err(|e| e.to_string())? {
            for import in imports.into_imports() {
                let import = import.map_err(|e| e.to_string())?;
                if matches!(import.ty, wasmparser::TypeRef::Func(_))
                    && !allowed.contains(&(import.module.to_string(), import.name.to_string()))
                {
                    return Err(format!(
                        "WASM C/C++ package requires unavailable function '{}.{}'; provide a portable implementation or move the platform operation behind a Dream @js binding",
                        import.module, import.name
                    ));
                }
            }
        }
    }
    Ok(())
}
