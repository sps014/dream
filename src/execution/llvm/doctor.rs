use crate::driver::toolchain::ToolchainConfig;
use crate::execution::native::{cc, host_library_dir};
use dream_abi::{host_capability::HostCapability, target::TargetSpec};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn run(
    config: Arc<ToolchainConfig>,
    target: Option<&str>,
    json_output: bool,
) -> Result<bool, String> {
    let spec = match target {
        Some(target) => TargetSpec::parse(target)?,
        None => TargetSpec::host(),
    };
    let mut errors = Vec::new();
    let mut tools = serde_json::Map::new();
    match super::resolve_llvm(&config) {
        Ok(llvm) => {
            if spec.is_windows() {
                match super::icon::resource_command(&llvm) {
                    Ok(command) => {
                        tools.insert("resource_compiler".into(), json!({"path": command.get_program().to_string_lossy(), "args": command.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>()}));
                    }
                    Err(error) => {
                        tools.insert(
                            "resource_compiler".into(),
                            json!({"available": false, "reason": error}),
                        );
                    }
                }
            }
            if !spec.can_link_on_host() && !llvm.tool("clang").is_file() {
                errors.push("cross builds require a full LLVM with clang; install the development LLVM toolchain".into());
            }
            for name in [
                "opt",
                "llc",
                "llvm-link",
                "llvm-dis",
                "llvm-ar",
                "clang",
                "wasm-ld",
                "llvm-profdata",
                "lldb-dap",
                "llvm-rc",
            ] {
                let path = llvm.tool(name);
                tools.insert(
                    name.into(),
                    json!({"path": path, "available": path.is_file()}),
                );
            }
        }
        Err(error) => errors.push(error),
    }
    match cc::resolve_existing_target_cc(&config, &spec) {
        Ok(cc) => {
            let command = cc.cc_command(&config, &spec)?;
            tools.insert("cc".into(), json!({"path": command.get_program().to_string_lossy(), "args": command.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>()}));
            match cc.cxx_command(&config, &spec) {
                Ok(command) => {
                    tools.insert("cxx".into(), json!({"path": command.get_program().to_string_lossy(), "args": command.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>()}));
                }
                Err(error) => {
                    tools.insert("cxx".into(), json!({"available": false, "reason": error}));
                }
            }
        }
        Err(error) => errors.push(error),
    }
    let host_dir = host_library_dir(&config, &[HostCapability::Core], &spec);
    if host_dir.is_none() {
        errors.push(format!(
            "missing core capability library for {}; install target-built libraries in {}",
            spec.triple,
            config
                .targets
                .join(spec.triple.to_string())
                .join("lib")
                .display()
        ));
    }
    if let Some(dir) = &host_dir {
        if let Err(error) =
            crate::execution::native::capability_abi::validate(dir, &[HostCapability::Core], &spec)
        {
            errors.push(error);
        }
        if spec.is_windows()
            && !dir
                .join(HostCapability::Core.import_library_name(&spec))
                .is_file()
        {
            errors.push("core capability import library is missing".into());
        }
    }
    let sdk_args = super::runtime::sysroot_args(&config, &spec);
    if let Some(root) = &config.sysroot {
        if !root.is_dir() {
            errors.push(format!("sysroot does not exist: {}", root.display()));
        }
    }
    for capability in HostCapability::ALL {
        let path = host_dir
            .as_ref()
            .map(|dir| dir.join(capability.library_name(&spec)));
        tools.insert(
            capability.link_name().into(),
            json!({"path": path, "available": path.as_ref().is_some_and(|p| p.is_file())}),
        );
    }
    if (spec.is_ios()
        || spec.is_android()
        || (spec.is_apple() && !spec.can_link_on_host())
        || (spec.is_msvc() && !cfg!(windows)))
        && sdk_args.is_empty()
    {
        errors.push("target SDK is required; set DREAM_SYSROOT to the platform SDK/NDK sysroot (or SDKROOT for Apple targets)".into());
    }
    if !spec.can_link_on_host() && !config.runtime_c.join("core/include/dream_core.h").is_file() {
        errors.push(
            "cross builds require runtime C sources; set DREAM_RUNTIME_C to runtime/c".into(),
        );
    }
    let healthy = errors.is_empty();
    let report = json!({"target": spec.triple.to_string(), "healthy": healthy, "configuration_hash": config.fingerprint(), "tools": tools,
        "paths": {"sdk_args": sdk_args, "developer_dir": config.developer_dir.as_ref().map(|s| s.to_string_lossy()), "xcrun": config.find_on_path("xcrun"), "runtime_c": config.runtime_c, "prefix": config.prefix, "targets": config.targets, "sysroot": config.sysroot, "sdkroot": config.sdkroot.as_ref().map(|s| s.to_string_lossy()), "toolchains": config.toolchains, "host_library_search": config.host_library_dirs(), "path": config.path}, "errors": errors});
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "target: {}\nconfiguration hash: {}",
            spec.triple,
            config.fingerprint()
        );
        for section in ["paths", "tools"] {
            if let Some(Value::Object(entries)) = report.get(section) {
                for (name, value) in entries {
                    println!("{name}: {value}");
                }
            }
        }
        for error in errors {
            println!("error: {error}");
        }
    }
    Ok(healthy)
}
