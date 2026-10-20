//! Standalone WASI command components, linked by the pinned SDK's Canonical ABI linker.

use std::path::Path;
use std::process::Command;

use crate::driver::compiler::LlvmRuntimeRequest;
use crate::driver::toolchain::ToolchainConfig;
use crate::driver::wasi::run_captured;

pub(super) fn link(
    config: &ToolchainConfig,
    ll: &Path,
    output: &Path,
    req: &LlvmRuntimeRequest,
) -> Result<(), String> {
    let environment = req
        .target
        .spec()
        .execution_environment
        .component()
        .ok_or("expected a component target")?;
    let sdk = config.locate_tool(
        dream_abi::toolchain::pinned_tool("wasi-sdk").ok_or("missing WASI SDK registry")?,
    )?;
    let clang = sdk.executable(&format!("bin/wasm32-{environment}-clang"));
    let mut paths = dream_mir::runtime::component_runtime_c_files(&config.runtime_c);
    let transport = config.runtime_c.join("sys/wasi/component");
    paths.extend(
        ["url.c", "http.c", "http_transport.c", "sockets.c"]
            .into_iter()
            .map(|name| transport.join(name)),
    );
    let bindings = transport.join(if environment == "wasip3" {
        "p3/dream_http_p3"
    } else {
        "p2/dream_http_p2"
    });
    paths.push(bindings.with_extension("c"));
    paths.push(config.runtime_c.join("vendor/yyjson/src/yyjson.c"));
    let uri = config.runtime_c.join("vendor/uriparser");
    let mut uri_sources = std::fs::read_dir(uri.join("src"))
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    uri_sources.retain(|path| path.extension().is_some_and(|extension| extension == "c"));
    uri_sources.sort();
    paths.extend(uri_sources);
    let linked = dream_mir::runtime::wasm32_linked_units(&config.runtime_c, req.need);
    paths.extend(linked.iter().map(|unit| unit.path.clone()));
    let mut inputs = crate::driver::rt_stamp::files_under(&config.runtime_c);
    inputs.push(clang.clone());
    let key = format!(
        "component-v1:{environment}:{:?}:{}:{}",
        req.wasm_opt,
        crate::driver::rt_stamp::content_fingerprint(inputs),
        config.fingerprint()
    );
    let directory = config
        .native_rt_cache_root()
        .join("components")
        .join(blake3::hash(key.as_bytes()).to_hex().as_str());
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(".lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let mut objects = vec![bindings.with_file_name(format!(
            "{}_component_type.o",
            bindings
                .file_name()
                .ok_or("missing bindings name")?
                .to_string_lossy()
        ))];
    let namespaces = Default::default();
    for (index, path) in paths.iter().enumerate() {
        let object = directory.join(format!("{index}.o"));
        let mut command = Command::new(&clang);
        command
            .args([
                "-c",
                "-g0",
                "-flto",
                "-DDREAM_WASM32",
                "-DDREAM_COMPONENT",
                "-mbulk-memory",
                "-msimd128",
            ])
            .args(["-DURI_NO_UNICODE", "-DURI_STATIC_BUILD"])
            .args(super::runtime::clang_level_flags(req.wasm_opt));
        for include in crate::driver::wasi::guest_include_dirs(&config.runtime_c)
            .into_iter()
            .chain([
                config.runtime_c.join("sys/native/include"),
                config.runtime_c.join("vendor/yyjson/src"),
                uri.join("include"),
                uri.join("src"),
            ])
        {
            command.arg("-I").arg(include);
        }
        if let Some(unit) = linked.iter().find(|unit| unit.path == *path) {
            for define in &unit.defines {
                command.arg(format!("-D{define}"));
            }
            for include in &unit.include_dirs {
                command.arg("-I").arg(include);
            }
        }
        if dream_mir::runtime::wasm32_unit_defines_libc(path) {
            command.arg("-fno-builtin");
        }
        command.arg(path);
        if path.extension().is_some_and(|extension| extension == "s") {
            command
                .arg("-Wno-unused-command-line-argument")
                .arg("-o")
                .arg(&object);
            run_captured(&mut command, "WASI runtime assembly")?;
            objects.push(object);
            continue;
        }
        let compiled = super::runtime_cache::compile(
            command,
            &config.native_rt_cache_root(),
            &object,
            config.compiler_environment.is_empty(),
            &namespaces,
        )?;
        objects.push(compiled.object);
    }
    let mut command = Command::new(&clang);
    command
        .args([
            "-flto",
            "-Wno-override-module",
            "-Wl,--gc-sections",
            "-Wl,--strip-debug",
        ])
        .args(super::runtime::clang_level_flags(req.wasm_opt));
    if let Some(stack) = crate::driver::wasi::stack_size_bytes() {
        command.arg(format!("-Wl,-z,stack-size={stack}"));
    }
    let (package_objects, libraries) =
        super::component_sources::compile(config, &clang, ll, req.wasm_opt)?;
    command.args(objects).args(package_objects).arg(ll);
    for library in libraries {
        command.arg(format!("-l{library}"));
    }
    command.arg("-o").arg(output);
    run_captured(&mut command, "WASI component linker")?;
    validate(output)
}

fn validate(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if !wasmparser::Parser::is_component(&bytes) {
        return Err("WASI linker did not emit a component".into());
    }
    let mut depth: usize = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        match payload.map_err(|e| e.to_string())? {
            wasmparser::Payload::ModuleSection { .. }
            | wasmparser::Payload::ComponentSection { .. } => depth += 1,
            wasmparser::Payload::End(_) => depth = depth.saturating_sub(1),
            wasmparser::Payload::ComponentImportSection(imports) if depth == 0 => {
                for import in imports {
                    let import = import.map_err(|e| e.to_string())?;
                    if !import.name.name.starts_with("wasi:") {
                        return Err(format!(
                            "standalone WASI output imports nonstandard capability {}",
                            import.name.name
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
