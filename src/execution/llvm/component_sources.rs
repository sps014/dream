//! Package sources use the component SDK's headers and libc, so no Preview 1 imports leak in.
use crate::driver::toolchain::ToolchainConfig;
use crate::driver::wasi::run_captured;
use crate::driver::wasm_opt::OptLevel;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn compile(
    config: &ToolchainConfig,
    clang: &Path,
    ll: &Path,
    opt: OptLevel,
) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    let abi = ll.with_extension("abi.json");
    let sets = crate::execution::native::native_c::read_c_sources_from_abi(&abi);
    let mut libraries = crate::execution::native::c_link::read_c_libs_from_abi(&abi);
    let directory = ll.with_extension("component-c");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let mut objects = Vec::new();
    let mut cxx = false;
    for (set_index, set) in sets.iter().enumerate() {
        if !set.frameworks.is_empty() {
            return Err(format!(
                "C/C++ set '{}' uses frameworks unavailable on WASI",
                set.name
            ));
        }
        for (index, source) in set.sources.iter().enumerate() {
            let cpp = crate::driver::native_sets::is_cxx_source(Path::new(source));
            cxx |= cpp;
            let object = directory.join(format!("{set_index}-{index}.o"));
            let mut command = Command::new(clang);
            command
                .args(["-c", "-g0", "-flto", "-fno-ident", "-frandom-seed=0"])
                .arg(opt.wasm_clang_opt_flag())
                .arg(if cpp { "-std=gnu++20" } else { "-std=gnu11" })
                .arg(format!(
                    "-ffile-prefix-map={source}=package/{set_index}/{index}"
                ));
            for include in &set.include {
                command.arg("-I").arg(include);
            }
            command
                .arg("-I")
                .arg(dream_mir::runtime::runtime_abi_include_dir(
                    &config.runtime_c,
                ));
            for define in &set.defines {
                command.arg(format!("-D{define}"));
            }
            command.args(&set.cflags).arg(source).arg("-o").arg(&object);
            run_captured(&mut command, &format!("WASI C/C++ set '{}'", set.name))?;
            objects.push(object);
        }
        libraries.extend(set.libs.iter().cloned());
    }
    let shim = crate::driver::compiler::c_shim_path(ll);
    if shim.is_file() {
        let object = directory.join("shim.o");
        let mut command = Command::new(clang);
        command
            .args(["-c", "-flto", "-ffreestanding", "-fno-builtin", "-w"])
            .arg(&shim)
            .arg("-o")
            .arg(&object);
        run_captured(&mut command, "WASI C ABI shim")?;
        objects.push(object);
    }
    if cxx {
        libraries.extend(["c++".into(), "c++abi".into()]);
    }
    libraries.sort();
    libraries.dedup();
    Ok((objects, libraries))
}
