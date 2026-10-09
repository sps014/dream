use super::runtime::{anchor_unit, build_anchor, disassemble, reduce_disassembly};
use super::tools::LlvmTools;
use crate::driver::compiler::{LlvmRuntimeRequest, RuntimeSignatures};
use crate::driver::rt_stamp;
use crate::driver::wasi::{GuestUnitOutput, guest_include_dirs, unit_command};
use crate::driver::wasm_opt::OptLevel;
use std::path::Path;

pub(super) fn load(
    tools: &LlvmTools,
    req: &LlvmRuntimeRequest,
) -> Result<RuntimeSignatures, String> {
    let spec = req.target.spec();
    let flavor = if spec.capabilities.linear_memory {
        super::wasm::flavor(req.threads)
    } else {
        "native"
    };
    if let super::bundle::RtDir::Prebuilt(dir) =
        super::bundle::rt_dir(&tools.config, flavor, OptLevel::O0, req.need)
    {
        let cache_path = super::bundle::prebuilt_file(&dir, "dream_rt.sigs")?;
        return Ok(RuntimeSignatures {
            text: std::fs::read_to_string(&cache_path).map_err(|e| e.to_string())?,
            cache_path,
        });
    }
    let clang = tools.clang()?;
    let mut inputs = rt_stamp::files_under(&tools.config.runtime_c);
    inputs.retain(|path| path.extension().is_some_and(|ext| ext == "h"));
    let clang_identity = rt_stamp::tool_identity(&clang).ok_or("unreadable clang identity")?;
    let dis_identity =
        rt_stamp::tool_identity(&tools.tool("llvm-dis")).ok_or("unreadable llvm-dis identity")?;
    let key = format!(
        "abi-v3:{spec:?}:{}:{}:{}:{clang_identity}:{dis_identity}:{:?}:{:?}",
        req.threads,
        tools.config.fingerprint(),
        rt_stamp::content_fingerprint(inputs),
        tools.bin,
        spec.capabilities
            .linear_memory
            .then_some(crate::driver::wasi::GUEST_FEATURES)
    );
    let dir = tools
        .config
        .native_rt_cache_root()
        .join("signatures")
        .join(blake3::hash(key.as_bytes()).to_hex().as_str());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(".lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let output = dir.join("runtime.sigs");
    let stamp = dir.join(".integrity");
    let dependencies_file = dir.join(".anchor-dependencies.json");
    let dependencies_stamp = dir.join(".dependencies-integrity");
    let previous_dependencies = std::fs::read(&dependencies_file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<std::path::PathBuf>>(&bytes).ok());
    if output.is_file()
        && tools.config.compiler_environment.is_empty()
        && rt_stamp::matches(&stamp, &rt_stamp::content_fingerprint(vec![output.clone()]))
        && previous_dependencies.as_ref().is_some_and(|paths| {
            rt_stamp::matches(
                &dependencies_stamp,
                &rt_stamp::content_fingerprint(paths.clone()),
            )
        })
    {
        return Ok(RuntimeSignatures {
            text: std::fs::read_to_string(&output).map_err(|e| e.to_string())?,
            cache_path: output,
        });
    }
    let anchor = if spec.capabilities.linear_memory {
        let sysroot = super::wasm::wasi_sysroot(&clang)?;
        let include_dirs = guest_include_dirs(&tools.config.runtime_c);
        let includes: Vec<_> = include_dirs.iter().map(|path| path.as_path()).collect();
        let command = |source: &Path| {
            let mut command = unit_command(
                &clang,
                &sysroot,
                source,
                &includes,
                &[],
                &[],
                req.threads,
                OptLevel::O0,
                GuestUnitOutput {
                    stable_name: "anchor.c",
                    bitcode: true,
                },
            );
            command.arg("-DDREAM_ALWAYS_INLINE=__attribute__((always_inline))");
            command
        };
        let check = |source: &Path| {
            command(source)
                .args([
                    "-fsyntax-only",
                    "-w",
                    "-ferror-limit=0",
                    "-fno-color-diagnostics",
                ])
                .arg(source)
                .output()
                .map(|out| String::from_utf8_lossy(&out.stderr).into_owned())
                .map_err(|e| e.to_string())
        };
        let compile = |source: &Path, output: &Path| {
            let mut command = command(source);
            command.arg("-w").arg(source);
            let dependencies = super::runtime_cache::compile(
                command,
                &tools.config.native_rt_cache_root(),
                output,
                tools.config.compiler_environment.is_empty(),
                &Default::default(),
            )?
            .dependencies;
            std::fs::write(
                &dependencies_file,
                serde_json::to_vec(&dependencies).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        };
        anchor_unit(&dir, "dream_rt_wasm32.h", &check, &compile)?
    } else {
        build_anchor(
            &tools.config,
            spec,
            &clang,
            &dir,
            &[
                "-emit-llvm",
                "-O0",
                "-DDREAM_ALWAYS_INLINE=__attribute__((always_inline))",
            ],
        )?
    };
    let ir = disassemble(tools, &anchor)?;
    let ir = if spec.capabilities.linear_memory {
        ir
    } else {
        super::runtime::strip_cpu_attrs(&ir)
    };
    let text = reduce_disassembly(&ir);
    let partial = dir.join("runtime.partial.sigs");
    std::fs::write(&partial, &text).map_err(|e| e.to_string())?;
    std::fs::rename(partial, &output).map_err(|e| e.to_string())?;
    std::fs::write(stamp, rt_stamp::content_fingerprint(vec![output.clone()]))
        .map_err(|e| e.to_string())?;
    let dependencies: Vec<std::path::PathBuf> =
        serde_json::from_slice(&std::fs::read(dependencies_file).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    std::fs::write(
        dependencies_stamp,
        rt_stamp::content_fingerprint(dependencies),
    )
    .map_err(|e| e.to_string())?;
    Ok(RuntimeSignatures {
        text,
        cache_path: output,
    })
}
