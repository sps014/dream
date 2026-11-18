use super::{Binding, Request, model};
use crate::driver::native_sets::NativeGraph;
use crate::driver::toolchain::ToolchainConfig;
use crate::execution::llvm::{bundle, resolve_llvm, runtime, wasm};
use dream_abi::c_abi::{is_c_identifier, is_link_name};
use dream_abi::target::TargetSpec;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

const CACHE_VERSION: &str = "cimport-v4";

pub(in crate::driver) struct Build<'a> {
    pub config: &'a Arc<ToolchainConfig>,
    pub native: &'a NativeGraph,
    pub optimize: crate::driver::wasm_opt::OptLevel,
    pub debug: bool,
}

impl Build<'_> {
    pub(super) fn generate(
        &self,
        request: &Request,
        metadata: &Path,
        target: &TargetSpec,
    ) -> Result<Binding, String> {
        let config = self.config;
        let c = request.import.c.as_ref().ok_or("missing C import")?;
        let header: String = serde_json::from_str(&request.import.module_name.text)
            .map_err(|_| "invalid header string")?;
        if header.is_empty() || header.contains(['"', '\\', '\n', '\r', '\0', '<', '>']) {
            return Err("invalid C header name".into());
        }
        let library = c
            .library
            .as_ref()
            .map(|t| serde_json::from_str::<String>(&t.text).map_err(|_| "invalid C library name"))
            .transpose()?;
        if library.as_ref().is_some_and(|name| {
            !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-+".contains(&b))
                || !is_link_name(name)
                || name.starts_with('-')
        }) {
            return Err("invalid C library name".into());
        }
        let tools = resolve_llvm(config)?;
        let clang = tools.clang()?;
        let mut args = vec!["-std=gnu11".into()];
        if target.capabilities.linear_memory {
            args.push(self.optimize.wasm_clang_opt_flag().into());
            let sysroot = config.sysroot.clone().map(Ok).unwrap_or_else(|| {
                bundle::prebuilt_rt(config)
                    .map(|root| Ok(root.join("wasi-sysroot")))
                    .unwrap_or_else(|| wasm::wasi_sysroot(&clang))
            })?;
            let triple = if target.llvm_triple().ends_with("threads") {
                "wasm32-wasip1-threads"
            } else {
                "wasm32-wasip1"
            };
            args.extend([
                format!("--target={triple}"),
                format!("--sysroot={}", dunce::simplified(&sysroot).display()),
            ]);
        } else {
            args.push(if self.debug { "-O0" } else { "-O2" }.into());
            args.push(format!("--target={}", target.llvm_triple()));
            args.extend(runtime::sysroot_args(config, target));
            if !target.is_windows() {
                args.push("-fPIC".into());
            }
        }

        let library = match library {
            Some(library) => library,
            None => {
                let name = self
                    .native
                    .implicit_set_for_file(&request.source)
                    .ok_or("a bundled C import needs a package with C sources under native/")?;
                let set = &self.native.sets[name];
                let relative = Path::new(&header);
                if !relative
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                    || !set.include.iter().any(|dir| dir.join(relative).is_file())
                {
                    return Err(format!(
                        "'{header}' was not found in this package's native include directories"
                    ));
                }
                args.extend(set.include.iter().map(|p| format!("-I{}", p.display())));
                args.extend(set.defines.iter().map(|d| format!("-D{d}")));
                args.extend(set.cflags.iter().cloned());
                name.to_owned()
            }
        };
        let include = format!("#include <{header}>\n");
        let output = run(&clang, &args, &["-E", "-dD", "-"], Some(&include))?;
        let preprocessed = checked_text(output)?;
        let fingerprint = fingerprint(
            &clang,
            &metadata.with_extension("clang"),
            &args,
            &library,
            &preprocessed,
        )?;
        if let Ok(bytes) = std::fs::read(metadata)
            && let Ok(binding) = serde_json::from_slice::<Binding>(&bytes)
            && binding.fingerprint == fingerprint
            && metadata.with_extension("dream").is_file()
        {
            tracing::debug!(header, "reusing C import bindings");
            return Ok(binding);
        }

        tracing::debug!(header, "generating C import bindings");
        let (path, macros, mut omissions) = header_macros(&preprocessed)?;
        let mut command = Command::new(&clang);
        let output = command
            .args(&args)
            .args([
                "-x",
                "c-header",
                "-fsyntax-only",
                "-Xclang",
                "-ast-dump=json",
            ])
            .arg(&path)
            .output()
            .map_err(|e| e.to_string())?;
        let ast: Value = serde_json::from_str(&checked_text(output)?)
            .map_err(|e| format!("invalid Clang AST: {e}"))?;
        let model = model::Header::new(
            &ast,
            !target.capabilities.linear_memory
                && !target.is_windows()
                && target.llvm_triple().contains("64"),
        );

        let mut probes = include.clone();
        for (_, probe) in model.enum_probes() {
            probes.push_str(&probe);
            probes.push('\n');
        }
        for (i, name) in macros.iter().enumerate() {
            use std::fmt::Write as _;
            let _ = writeln!(probes, "enum {{ __dream_macro_{i} = ({name}) }};");
        }
        let mut values = model::constant_values(&ast);
        if probes.len() != include.len() {
            let output = run(
                &clang,
                &args,
                &[
                    "-fsyntax-only",
                    "-ferror-limit=0",
                    "-Xclang",
                    "-ast-dump=json",
                    "-",
                ],
                Some(&probes),
            )?;
            // Noninteger macros fail the constant-expression probe independently of valid macros.
            if let Ok(ast) = serde_json::from_slice(&output.stdout) {
                values.extend(model::constant_values(&ast));
            }
        }
        let mut api = model.api(&values, &macros);
        let probes = include + &api.probes();
        let ir = checked_text(run(
            &clang,
            &args,
            &["-fno-builtin", "-S", "-emit-llvm", "-o", "-", "-"],
            Some(&probes),
        )?)?;
        api.verify(&ir);
        omissions.extend(api.omissions.clone());
        let binding = Binding {
            fingerprint,
            source: api.render(&c.prefix, &library),
            omissions,
        };
        publish(&metadata.with_extension("dream"), binding.source.as_bytes())?;
        let json = serde_json::to_vec(&binding).map_err(|e| e.to_string())?;
        publish(metadata, &json)?;
        Ok(binding)
    }
}

fn run(
    clang: &Path,
    args: &[String],
    extra: &[&str],
    input: Option<&str>,
) -> Result<Output, String> {
    let mut command = Command::new(clang);
    command.args(args).args(["-x", "c"]).args(extra);
    if let Some(input) = input {
        let mut file = tempfile::tempfile().map_err(|e| e.to_string())?;
        file.write_all(input.as_bytes())
            .map_err(|e| e.to_string())?;
        std::io::Seek::rewind(&mut file).map_err(|e| e.to_string())?;
        command.stdin(Stdio::from(file));
    }
    command
        .output()
        .map_err(|e| format!("running {}: {e}", clang.display()))
}

fn checked_text(output: Output) -> Result<String, String> {
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn fingerprint(
    clang: &Path,
    stamp: &Path,
    args: &[String],
    library: &str,
    preprocessed: &str,
) -> Result<String, String> {
    let identity = crate::driver::rt_stamp::persistent_tool_identity(clang, stamp)
        .ok_or("cannot read clang identity")?;
    let mut hash = blake3::Hasher::new();
    for part in [
        CACHE_VERSION,
        &identity,
        &format!("{args:?}"),
        library,
        preprocessed,
    ] {
        hash.update(part.as_bytes());
        hash.update(b"\0");
    }
    Ok(hash.finalize().to_hex().to_string())
}

type HeaderMacros = (PathBuf, Vec<String>, BTreeMap<String, String>);

fn header_macros(text: &str) -> Result<HeaderMacros, String> {
    let mut source = None;
    let mut current = String::new();
    let mut macros = BTreeSet::new();
    let mut omissions = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with("# ")
            && let Some((_, quoted)) = line.split_once('"')
            && let Some((path, _)) = quoted.rsplit_once('"')
        {
            current.clear();
            current.push_str(path);
            if source.is_none() && !path.starts_with('<') {
                source = Some(path.to_owned());
            }
            continue;
        }
        if source.as_deref() != Some(&current) {
            continue;
        }
        if let Some(decl) = line.strip_prefix("#define ") {
            let end = decl
                .find(|c: char| c.is_whitespace() || c == '(')
                .unwrap_or(decl.len());
            let (name, body) = decl.split_at(end);
            if !is_c_identifier(name) {
                continue;
            }
            if body.starts_with('(') {
                macros.remove(name);
                omissions.insert(
                    name.to_owned(),
                    "function-like C macros are not supported".into(),
                );
            } else if !body.trim().is_empty() {
                omissions.remove(name);
                macros.insert(name.to_owned());
            }
        } else if let Some(name) = line.strip_prefix("#undef ") {
            macros.remove(name);
            omissions.remove(name);
        }
    }
    let path = source.ok_or("clang did not resolve the requested header")?;
    Ok((PathBuf::from(path), macros.into_iter().collect(), omissions))
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let dir = path.parent().ok_or("invalid C binding cache path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
