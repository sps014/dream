//! The C runtime as LLVM bitcode. A release reads it prebuilt from `lib/dream/rt/native`; a
//! development build compiles it with the dev LLVM's clang into the same layout under the
//! native runtime cache.
//!
//! `dream_rt.bc` holds the core units, `core/inlines.c` (external definitions of the header's
//! always-inline helpers) and each needed module's own wrapper (`regex.c`), so `opt` sees every
//! runtime function the program calls. Vendored libraries (PCRE2) only call libc and stay in a
//! native archive. `dream_rt.sigs` is the reduced disassembly the backend types runtime calls
//! from.
//!
//! The bitcode carries no `target-cpu`/`target-features`: the machine that built it is not the
//! one that runs it, so the program's `-mcpu` (see `build::cpu_args`) decides for both.

use super::bundle::{RtDir, prebuilt_file, rt_dir};
use super::tools::LlvmTools;
use crate::driver::rt_stamp;
use crate::driver::wasi::run_captured;
use crate::driver::wasm_opt::OptLevel;
use dream_abi::target::TargetSpec;
use dream_mir::runtime::{RUNTIME_MODULES, RuntimeNeed, core_runtime_include_dir};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct LlvmRuntime {
    pub bc: PathBuf,
    pub sigs: PathBuf,
    /// Vendored C for the needed modules, compiled natively (`None` when nothing is needed).
    pub archive: Option<PathBuf>,
}

const VENDOR_ARCHIVE: &str = "libdream_rt_vendor.a";

fn snapshot_runtime(
    dir: &Path,
    bc: PathBuf,
    sigs: PathBuf,
    archive: Option<PathBuf>,
) -> Result<LlvmRuntime, String> {
    let files: Vec<_> = [Some(bc), Some(sigs), archive]
        .into_iter()
        .flatten()
        .collect();
    let outputs = super::runtime_snapshot::publish(dir, &files)?;
    Ok(LlvmRuntime {
        bc: outputs[0].clone(),
        sigs: outputs[1].clone(),
        archive: outputs.get(2).cloned(),
    })
}

pub(super) struct Unit {
    path: PathBuf,
    defines: Vec<String>,
    include_dirs: Vec<PathBuf>,
}

pub(super) fn clang_level_flags(opt: OptLevel) -> Vec<&'static str> {
    match opt {
        OptLevel::O0 => vec!["-O0"],
        OptLevel::O1 => vec!["-O1"],
        OptLevel::O2 => vec!["-O2"],
        OptLevel::O3 | OptLevel::O4 => vec!["-O3"],
        OptLevel::Size => vec!["-Os"],
        OptLevel::SizeAggressive => vec!["-Oz"],
    }
}

/// `-isysroot` for the pinned clang on macOS, which (unlike Apple's) doesn't find the SDK itself.
pub(crate) fn sysroot_args(
    config: &crate::driver::toolchain::ToolchainConfig,
    spec: &TargetSpec,
) -> Vec<String> {
    if let Some(root) = &config.sysroot {
        return vec![format!("--sysroot={}", root.display())];
    }
    if !spec.is_apple() {
        return Vec::new();
    }
    let mut cached = config
        .sdkroot_args
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cached
        .entry(spec.triple.to_string())
        .or_insert_with(|| {
            let sdk = config
                .sdkroot
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .or_else(|| {
                    let mut command = Command::new(config.find_on_path("xcrun")?);
                    if let Some(dir) = &config.developer_dir {
                        command.env("DEVELOPER_DIR", dir);
                    } else {
                        command.env_remove("DEVELOPER_DIR");
                    }
                    let out = command
                        .args([
                            "--sdk",
                            if spec.is_ios() {
                                if spec.triple.to_string().ends_with("-sim") {
                                    "iphonesimulator"
                                } else {
                                    "iphoneos"
                                }
                            } else {
                                "macosx"
                            },
                            "--show-sdk-path",
                        ])
                        .output()
                        .ok()?;
                    out.status
                        .success()
                        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
                });
            sdk.map(|s| vec!["-isysroot".to_string(), s])
                .unwrap_or_default()
        })
        .clone()
}

pub(super) fn runtime_command(
    config: &crate::driver::toolchain::ToolchainConfig,
    spec: &TargetSpec,
    clang: &Path,
) -> Result<Command, String> {
    let mut command = if spec.can_link_on_host() {
        let mut command = Command::new(clang);
        command.arg(native_target_arg(spec));
        command
    } else {
        crate::execution::native::cc::resolve_target_cc(config, spec)?.cc_command(config, spec)?
    };
    command.args(sysroot_args(config, spec));
    Ok(command)
}

/// Clang's default may describe its runner rather than the compiler's selected target.
fn native_target_arg(spec: &TargetSpec) -> String {
    format!("--target={}", spec.llvm_triple())
}

pub(super) fn bitcode_units(root: &Path, need: RuntimeNeed) -> (Vec<Unit>, Vec<Unit>) {
    let c = root.to_path_buf();
    let native_inc = core_runtime_include_dir(root);
    let mut bc: Vec<Unit> = dream_mir::runtime::native_runtime_units(root, RuntimeNeed::CORE)
        .into_iter()
        .map(|u| Unit {
            path: u.path,
            defines: u.defines,
            include_dirs: u.include_dirs,
        })
        .collect();
    let mut vendored = Vec::new();
    for m in RUNTIME_MODULES {
        if m.need == RuntimeNeed::CORE || !need.contains(m.need) {
            continue;
        }
        let mut dirs = vec![native_inc.clone(), c.join("include")];
        for rel in m.include_dirs {
            let d = c.join(rel);
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
        let defines: Vec<String> = m.native_defines.iter().map(|s| (*s).to_string()).collect();
        let shared: Vec<PathBuf> = m.shared_c.iter().map(|r| c.join(r)).collect();
        for u in dream_mir::runtime::native_runtime_units(root, m.need) {
            let unit = Unit {
                path: u.path,
                defines: defines.clone(),
                include_dirs: dirs.clone(),
            };
            if shared.contains(&unit.path) {
                bc.push(unit);
            } else if !bc.iter().any(|b| b.path == unit.path) {
                vendored.push(unit);
            }
        }
    }
    (bc, vendored)
}

pub(super) fn clang_unit(
    config: &crate::driver::toolchain::ToolchainConfig,
    spec: &TargetSpec,
    clang: &Path,
    u: &Unit,
    flags: &[&str],
    out: &Path,
    namespaces: &super::runtime_cache::IncludeInventory,
) -> Result<super::runtime_cache::CompiledUnit, String> {
    let mut cmd = runtime_command(config, spec, clang)?;
    if config.runtime_counters {
        cmd.arg("-DDREAM_RUNTIME_COUNTERS=1");
    }
    cmd.args(["-std=gnu11", "-w", "-c"])
        .args(if spec.is_windows() {
            &[][..]
        } else {
            &["-pthread"][..]
        })
        .args(flags);
    for inc in &u.include_dirs {
        cmd.arg(format!("-I{}", inc.display()));
    }
    for d in &u.defines {
        cmd.arg(format!("-D{d}"));
    }
    cmd.arg(&u.path);
    super::runtime_cache::compile(
        cmd,
        &config.native_rt_cache_root(),
        out,
        config.compiler_environment.is_empty(),
        namespaces,
    )
}

pub fn llvm_runtime(
    tools: &LlvmTools,
    spec: &TargetSpec,
    opt: OptLevel,
    need: RuntimeNeed,
    _debug: bool,
) -> Result<LlvmRuntime, String> {
    match rt_dir(&tools.config, "native", opt, need) {
        RtDir::Prebuilt(_) if !spec.can_link_on_host() => build_native_runtime(
            tools,
            spec,
            opt,
            need,
            &tools
                .config
                .native_rt_cache_root()
                .join("cross")
                .join(spec.triple.to_string())
                .join(opt.native_rt_subdir())
                .join(format!("need_{:x}", need.bits())),
        ),
        RtDir::Prebuilt(dir) => Ok(LlvmRuntime {
            bc: prebuilt_file(&dir, "dream_rt.bc")?,
            sigs: prebuilt_file(&dir, "dream_rt.sigs")?,
            archive: Some(dir.join(VENDOR_ARCHIVE)).filter(|a| a.is_file()),
        }),
        RtDir::Cache(dir) => {
            build_native_runtime(tools, spec, opt, need, &dir.join(spec.triple.to_string()))
        }
    }
}

/// Compiles the native runtime for `opt`/`need` into `dir`, unless its stamp says it is current.
pub(super) fn build_native_runtime(
    tools: &LlvmTools,
    spec: &TargetSpec,
    opt: OptLevel,
    need: RuntimeNeed,
    dir: &Path,
) -> Result<LlvmRuntime, String> {
    let clang = tools.clang()?;
    let io = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::create_dir_all(dir).map_err(io)?;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(".lock"))
        .map_err(io)?;
    lock_file.lock().map_err(io)?;

    let config = &tools.config;
    let root = &config.runtime_c;
    let (bc_units, vendored) = bitcode_units(root, need);
    let bc = dir.join("dream_rt.bc");
    let sigs = dir.join("dream_rt.sigs");
    let archive = (!vendored.is_empty()).then(|| dir.join(VENDOR_ARCHIVE));
    let stamp = dir.join(".stamp");
    let mut level = clang_level_flags(opt);
    // The same bitcode backs shared libraries, whose ELF TLS cannot use local-exec relocations.
    if !spec.is_windows() {
        level.push("-fPIC");
    }
    let mut inputs: Vec<PathBuf> = rt_stamp::files_under(root);
    inputs.push(clang.clone());
    inputs.push(tools.tool("llvm-link"));
    inputs.push(tools.tool("llvm-dis"));
    if !spec.can_link_on_host() {
        inputs.push(
            crate::execution::native::cc::resolve_target_cc(config, spec)?
                .path()
                .to_path_buf(),
        );
    }
    let dependency_file = dir.join(".dependencies.json");
    let previous_dependencies = std::fs::read(&dependency_file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok());
    let fingerprint_for = |dependencies: &[PathBuf]| {
        let mut all = inputs.clone();
        all.extend_from_slice(dependencies);
        format!(
            "{}{}\n{}\n{}|{}\n",
            rt_stamp::content_fingerprint(all),
            level.join(" "),
            native_target_arg(spec),
            sysroot_args(config, spec).join(" "),
            config.fingerprint()
        )
    };
    let fingerprint = fingerprint_for(previous_dependencies.as_deref().unwrap_or_default());
    let output_stamp = dir.join(".outputs");
    let outputs: Vec<PathBuf> = [Some(bc.clone()), Some(sigs.clone()), archive.clone()]
        .into_iter()
        .flatten()
        .collect();
    let fresh = rt_stamp::matches(
        &output_stamp,
        &rt_stamp::content_fingerprint(outputs.clone()),
    ) && previous_dependencies.is_some()
        && bc.exists()
        && sigs.exists()
        && archive.as_ref().is_none_or(|a| a.exists())
        && rt_stamp::matches(&stamp, &fingerprint);
    if fresh && config.compiler_environment.is_empty() {
        return snapshot_runtime(dir, bc, sigs, archive);
    }

    let mut bc_flags = vec!["-emit-llvm"];
    if opt != OptLevel::O0 {
        bc_flags.push("-flto=full");
    }
    bc_flags.extend(&level);
    let namespaces = Default::default();
    let mut dependencies = Vec::new();
    let mut parts = Vec::new();
    for (i, u) in bc_units.iter().enumerate() {
        let out = dir.join(format!("{i}.bc"));
        dependencies.extend(
            clang_unit(config, spec, &clang, u, &bc_flags, &out, &namespaces)?.dependencies,
        );
        parts.push(out);
    }
    let mut link = tools.command("llvm-link");
    link.args(&parts).arg("-o").arg(&bc);
    run_captured(&mut link, "llvm-link (runtime)")?;
    strip_target_cpu(tools, &bc)?;

    let anchor = build_anchor(config, spec, &clang, dir, &bc_flags)?;
    let merged = dir.join("sigs.bc");
    let mut link = tools.command("llvm-link");
    link.arg(&bc).arg(&anchor).arg("-o").arg(&merged);
    run_captured(&mut link, "llvm-link (runtime signatures)")?;
    let merged_ll = strip_cpu_attrs(&disassemble(tools, &merged)?);
    std::fs::write(&sigs, reduce_disassembly(&merged_ll)).map_err(io)?;

    if let Some(archive) = &archive {
        let mut objs = Vec::new();
        for (i, u) in vendored.iter().enumerate() {
            let obj = dir.join(format!("v{i}.o"));
            dependencies.extend(
                clang_unit(config, spec, &clang, u, &level, &obj, &namespaces)?.dependencies,
            );
            objs.push(obj);
        }
        let _ = std::fs::remove_file(archive);
        let mut ar = tools.command("llvm-ar");
        ar.arg("crs").arg(archive).args(&objs);
        run_captured(&mut ar, "llvm-ar (runtime)")?;
        for o in &objs {
            let _ = std::fs::remove_file(o);
        }
    }
    for p in parts.iter().chain([&anchor, &merged]) {
        let _ = std::fs::remove_file(p);
    }
    std::fs::write(
        &dependency_file,
        serde_json::to_vec(&dependencies).map_err(|e| e.to_string())?,
    )
    .map_err(io)?;
    std::fs::write(&output_stamp, rt_stamp::content_fingerprint(outputs)).map_err(io)?;
    std::fs::write(&stamp, fingerprint_for(&dependencies)).map_err(io)?;
    snapshot_runtime(dir, bc, sigs, archive)
}

pub(super) fn disassemble(tools: &LlvmTools, bc: &Path) -> Result<String, String> {
    let out = tools
        .command("llvm-dis")
        .arg(bc)
        .arg("-o")
        .arg("-")
        .output()
        .map_err(|e| format!("llvm-dis: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "llvm-dis failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Rewrites `bc` without the CPU clang tuned it for (textual IR round-trips through llvm-link).
pub(super) fn strip_target_cpu(tools: &LlvmTools, bc: &Path) -> Result<(), String> {
    let ll = bc.with_extension("ll");
    std::fs::write(&ll, strip_cpu_attrs(&disassemble(tools, bc)?))
        .map_err(|e| format!("{}: {e}", ll.display()))?;
    let mut link = tools.command("llvm-link");
    link.arg(&ll).arg("-o").arg(bc);
    let r = run_captured(&mut link, "llvm-link (runtime cpu strip)");
    let _ = std::fs::remove_file(&ll);
    r
}

const CPU_ATTRS: [&str; 3] = ["target-cpu", "target-features", "tune-cpu"];

pub(super) fn strip_cpu_attrs(ll: &str) -> String {
    let mut out = String::with_capacity(ll.len());
    for line in ll.lines() {
        if line.starts_with("attributes #") {
            let mut line = line.to_string();
            for key in CPU_ATTRS {
                let pat = format!(" \"{key}\"=\"");
                if let Some(at) = line.find(&pat) {
                    let value = at + pat.len();
                    if let Some(len) = line[value..].find('"') {
                        line.replace_range(at..value + len + 1, "");
                    }
                }
            }
            out.push_str(&line);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

pub(super) fn build_anchor(
    config: &crate::driver::toolchain::ToolchainConfig,
    spec: &TargetSpec,
    clang: &Path,
    dir: &Path,
    flags: &[&str],
) -> Result<PathBuf, String> {
    let root = &config.runtime_c;
    let inc = format!("-I{}", core_runtime_include_dir(root).display());
    let anchor_command = || -> Result<Command, String> {
        if spec.can_link_on_host() {
            runtime_command(config, spec, clang)
        } else {
            let mut cmd = Command::new(clang);
            cmd.arg(native_target_arg(spec)).arg("-ffreestanding");
            Ok(cmd)
        }
    };
    let check = |src: &Path| {
        anchor_command()?
            .args([
                "-std=gnu11",
                "-w",
                "-ferror-limit=0",
                "-fsyntax-only",
                "-DDREAM_NATIVE",
            ])
            .arg(&inc)
            .arg(src)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
            .map_err(|e| e.to_string())
    };
    let compile = |src: &Path, out: &Path| {
        let mut command = anchor_command()?;
        command
            .args(["-std=gnu11", "-w", "-c", "-DDREAM_NATIVE"])
            .args(flags)
            .arg(&inc)
            .arg(src);
        let dependencies = super::runtime_cache::compile(
            command,
            &config.native_rt_cache_root(),
            out,
            config.compiler_environment.is_empty(),
            &Default::default(),
        )?
        .dependencies;
        std::fs::write(
            dir.join(".anchor-dependencies.json"),
            serde_json::to_vec(&dependencies).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    };
    anchor_unit(dir, "dream_core.h", &check, &compile)
}

/// A unit that takes the address of every header-declared function, so functions only the
/// generated code calls (host exports from capability libraries, wasm host imports) still appear with clang's
/// lowering. Some header names are macros or builtins; `check` (a syntax-only compile returning
/// its stderr, uncoloured so its `file:line:` prefixes parse) finds their lines, which are dropped
/// before `compile` builds the bitcode.
pub(super) fn anchor_unit(
    dir: &Path,
    header: &str,
    check: &dyn Fn(&Path) -> Result<String, String>,
    compile: &dyn Fn(&Path, &Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let names = dream_mir::backend::llvm::native_header_function_names();
    let src = dir.join("anchor.c");
    let write = |keep: &dyn Fn(usize) -> bool| -> std::io::Result<()> {
        let mut text = format!("#include \"{header}\"\n");
        for (i, n) in names.iter().enumerate() {
            if keep(i + 2) {
                text.push_str(&format!("void *__dream_anchor_{n} = (void *)&{n};\n"));
            } else {
                text.push('\n');
            }
        }
        text.push_str("void *__dream_anchor_defer(void) { return (void *)&dream_defer_open; }\n");
        std::fs::write(&src, text)
    };
    write(&|_| true).map_err(|e| e.to_string())?;
    let prefix = format!("{}:", src.display());
    let bad: std::collections::BTreeSet<usize> = check(&src)?
        .lines()
        .filter(|l| l.contains(": error:"))
        .filter_map(|l| l.strip_prefix(&prefix)?.split(':').next()?.parse().ok())
        .collect();
    write(&|line| !bad.contains(&line)).map_err(|e| e.to_string())?;
    let rest = check(&src)?;
    if rest.contains("error:") {
        return Err(format!(
            "runtime anchor unit still fails to compile:\n{rest}"
        ));
    }
    let obj = dir.join("anchor.bc");
    compile(&src, &obj)?;
    Ok(obj)
}

/// Keeps what `RuntimeSigs::parse` reads, so each compile parses kilobytes, not the full module.
pub(super) fn reduce_disassembly(ll: &str) -> String {
    let mut out = String::new();
    for line in ll.lines() {
        let keep = line.starts_with("target ")
            || line.starts_with("attributes #")
            || line.starts_with("declare ")
            || (line.starts_with('@')
                && !line.starts_with("@__dream_anchor_")
                && !line.starts_with("@."))
            || line.starts_with("define ");
        if keep {
            let line = line.strip_suffix(" {").unwrap_or(line);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_mir::backend::llvm::RuntimeSigs;
    use dream_mir::backend::llvm::ir::Ty;

    #[test]
    fn runtime_bitcode_signatures_cover_header() {
        let Ok(tools) = super::super::resolve_llvm(&std::sync::Arc::new(
            crate::driver::toolchain::ToolchainConfig::default(),
        )) else {
            eprintln!("skipping: pinned LLVM not installed");
            return;
        };
        let rt = llvm_runtime(
            &tools,
            &TargetSpec::host(),
            OptLevel::O2,
            RuntimeNeed::CORE,
            false,
        )
        .expect("runtime");
        let sigs =
            RuntimeSigs::parse(&std::fs::read_to_string(&rt.sigs).expect("sigs")).expect("parse");
        assert!(sigs.function("dream_panic").noreturn);
        assert_eq!(sigs.function("dream_retain").fty.params, vec![Ty::Ptr]);
        assert_eq!(sigs.function("dream_malloc").fty.ret, Ty::Ptr);
        assert!(sigs.globals["g0"].thread_local);
        assert!(
            sigs.target_attrs
                .iter()
                .all(|(k, _)| !CPU_ATTRS.contains(&k.as_str()))
        );
    }

    #[test]
    fn cpu_attrs_are_stripped_from_attribute_groups_only() {
        let ll = "define void @f() #0 {\n\
                  attributes #0 = { nounwind \"frame-pointer\"=\"non-leaf\" \"target-cpu\"=\"apple-m1\" \"target-features\"=\"+neon,+v8a\" \"tune-cpu\"=\"generic\" }\n\
                  @s = constant [11 x i8] c\"target-cpu\\00\"\n";
        assert_eq!(
            strip_cpu_attrs(ll),
            "define void @f() #0 {\n\
             attributes #0 = { nounwind \"frame-pointer\"=\"non-leaf\" }\n\
             @s = constant [11 x i8] c\"target-cpu\\00\"\n"
        );
    }
}
