//! `dreamer pack`: release-compile a bin package and embed the `.cwasm` in `dream-runner`.

use crate::manifest::{PackageType, RunTarget};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Supported pack triples (Dream name → rustc target).
const PACK_TRIPLES: &[(&str, &str)] = &[
    ("linux-x64", "x86_64-unknown-linux-gnu"),
    ("linux-arm64", "aarch64-unknown-linux-gnu"),
    ("macos-x64", "x86_64-apple-darwin"),
    ("macos-arm64", "aarch64-apple-darwin"),
    ("windows-x64", "x86_64-pc-windows-msvc"),
    ("windows-arm64", "aarch64-pc-windows-msvc"),
];

pub fn run(start_dir: &Path, target_args: &[String], package: Option<&str>) -> Result<()> {
    super::install::run(start_dir)?;
    let workspace = Workspace::discover_package(start_dir, package)?;
    let pkg = workspace.manifest.package()?;
    if pkg.package_type == PackageType::Lib {
        bail!(
            "package '{}' is type = \"lib\" and cannot be packed (only bin packages produce \
             native executables)",
            pkg.name
        );
    }

    let triples = resolve_pack_targets(target_args)?;
    super::build::compile_entry(
        &workspace,
        &crate::compile_flags::CompileFlags {
            release: true,
            ..crate::compile_flags::CompileFlags::default()
        },
        Some(RunTarget::Native),
    )?;

    let wasm_path = artifact_wasm_path(&workspace)?;
    if !wasm_path.is_file() {
        bail!(
            "expected release wasm at {} after build; was the entry compiled?",
            wasm_path.display()
        );
    }

    let c_libs = read_c_libs_from_abi(&wasm_path);

    let dream_root = find_dream_workspace_root().context(
        "could not locate the Dream workspace (need tooling/dream-runner). \
                  Set DREAM_REPO to the Dream checkout root, or run pack from a tree that \
                  includes the compiler workspace",
    )?;

    let pack_dir = workspace.root.join("target").join("pack");
    std::fs::create_dir_all(&pack_dir)
        .with_context(|| format!("creating {}", pack_dir.display()))?;

    let dream_bin = crate::dream_bin::locate()?;
    let host_triple = host_rustc_triple()?;
    let wasm_stem = wasm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("wasm path has no file stem"))?;

    let pkg_name = pkg.name.clone();
    for (dream_triple, rust_triple) in &triples {
        let cwasm_path = if rust_triple.as_str() == host_triple {
            let host_cwasm = wasm_path.with_extension("cwasm");
            if !host_cwasm.is_file() {
                run_dream_aot(&dream_bin, &wasm_path, &host_cwasm, None)?;
            }
            host_cwasm
        } else {
            let cross = pack_dir.join(format!("{wasm_stem}.{dream_triple}.cwasm"));
            run_dream_aot(&dream_bin, &wasm_path, &cross, Some(rust_triple))?;
            cross
        };
        if !cwasm_path.is_file() {
            bail!(
                "expected Cranelift AOT artifact at {} after `dream aot`",
                cwasm_path.display()
            );
        }
        let out_name = if dream_triple.starts_with("windows-") {
            format!("{pkg_name}-{dream_triple}.exe")
        } else {
            format!("{pkg_name}-{dream_triple}")
        };
        let dest = pack_dir.join(&out_name);
        let icon_path = resolve_package_icon(&workspace);
        let abi_path = wasm_path.with_extension("abi.json");
        build_runner(
            &dream_root,
            &cwasm_path,
            abi_path.as_path(),
            icon_path.as_deref(),
            rust_triple,
            &dest,
            &c_libs,
        )?;
        println!("packed {}", dest.display());
    }
    Ok(())
}

fn resolve_package_icon(workspace: &Workspace) -> Option<PathBuf> {
    let rel = workspace.manifest.package.as_ref()?.icon.as_ref()?;
    let path = workspace.root.join(rel);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!(
            "warning: package.icon '{}' not found at {}; packing without an app icon",
            rel,
            path.display()
        );
        None
    }
}

fn resolve_pack_targets(args: &[String]) -> Result<Vec<(String, String)>> {
    if args.is_empty() {
        let host = host_pack_triple()?;
        let rust = rust_triple_for(&host)
            .ok_or_else(|| anyhow::anyhow!("internal: unknown host pack triple {host}"))?;
        return Ok(vec![(host, rust.to_string())]);
    }

    let mut out = Vec::new();
    for arg in args {
        if arg == "all" {
            for &(d, r) in PACK_TRIPLES {
                if !out.iter().any(|(x, _)| x == d) {
                    out.push((d.to_string(), r.to_string()));
                }
            }
            continue;
        }
        let rust = rust_triple_for(arg).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown pack target '{}'; expected one of {} or 'all'",
                arg,
                PACK_TRIPLES
                    .iter()
                    .map(|(d, _)| *d)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        if !out.iter().any(|(x, _)| x == arg) {
            out.push((arg.clone(), rust.to_string()));
        }
    }
    Ok(out)
}

fn rust_triple_for(dream_triple: &str) -> Option<&'static str> {
    PACK_TRIPLES
        .iter()
        .find(|(d, _)| *d == dream_triple)
        .map(|(_, r)| *r)
}

fn host_pack_triple() -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let dream_os = match os {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => bail!("unsupported host OS for pack: {other}"),
    };
    let dream_arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => bail!("unsupported host arch for pack: {other}"),
    };
    Ok(format!("{dream_os}-{dream_arch}"))
}

fn run_dream_aot(
    dream_bin: &Path,
    wasm_path: &Path,
    cwasm_path: &Path,
    target: Option<&str>,
) -> Result<()> {
    let mut cmd = Command::new(dream_bin);
    cmd.arg("aot").arg(wasm_path).arg(cwasm_path);
    if let Some(triple) = target {
        cmd.arg("--target").arg(triple);
    }
    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("running {} aot: {e}", dream_bin.display()))?;
    if !status.success() {
        bail!(
            "dream aot failed for {} (exit {:?}). Cranelift must be able to compile for that \
             target; missing backends are not silently replaced with host .cwasm",
            target.unwrap_or("host"),
            status.code()
        );
    }
    Ok(())
}

fn artifact_wasm_path(workspace: &Workspace) -> Result<PathBuf> {
    let entry = workspace.compile_root_path()?;
    let stem = entry
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("entry has no file stem"))?;
    Ok(workspace
        .root
        .join("target")
        .join("web")
        .join(format!("{stem}.wasm")))
}

/// Reads `c_libs` from the sibling `.abi.json` produced next to the release wasm (auto-link set).
fn read_c_libs_from_abi(wasm_path: &Path) -> Vec<String> {
    let abi_path = wasm_path.with_extension("abi.json");
    let Ok(text) = std::fs::read_to_string(&abi_path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    v.get("c_libs")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn find_dream_workspace_root() -> Option<PathBuf> {
    if let Ok(repo) = std::env::var("DREAM_REPO") {
        let p = PathBuf::from(repo);
        if p.join("tooling")
            .join("dream-runner")
            .join("Cargo.toml")
            .is_file()
        {
            return Some(p);
        }
    }

    // Walk from the dream binary (or cwd) upward looking for the workspace Cargo.toml that lists
    // dream-runner.
    let mut starts = Vec::new();
    if let Ok(bin) = crate::dream_bin::locate() {
        if let Some(parent) = bin.parent() {
            starts.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            starts.push(parent.to_path_buf());
        }
    }

    for start in starts {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let candidate = d.join("tooling").join("dream-runner").join("Cargo.toml");
            if candidate.is_file() {
                return Some(d);
            }
            // Also accept being inside tooling/dreamer/target/...
            if d.join("Cargo.toml").is_file()
                && d.join("tooling")
                    .join("dream-runner")
                    .join("Cargo.toml")
                    .is_file()
            {
                return Some(d);
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }
    None
}

fn build_runner(
    dream_root: &Path,
    cwasm_path: &Path,
    abi_path: &Path,
    icon_path: Option<&Path>,
    rust_triple: &str,
    dest: &Path,
    c_libs: &[String],
) -> Result<()> {
    let host_triple = host_rustc_triple()?;
    let mut cmd = Command::new("cargo");
    cmd.current_dir(dream_root)
        .env("DREAM_EMBEDDED_WASM", cwasm_path)
        .args([
            "build",
            "-p",
            "dream-runner",
            "--release",
            "--manifest-path",
        ])
        .arg(dream_root.join("Cargo.toml"));

    if abi_path.is_file() {
        cmd.env("DREAM_EMBEDDED_ABI", abi_path);
    }
    if let Some(icon) = icon_path {
        cmd.env("DREAM_EMBEDDED_ICON", icon);
    }
    if !c_libs.is_empty() {
        cmd.env("DREAM_C_LIBS", c_libs.join(","));
    }

    if rust_triple != host_triple {
        cmd.arg("--target").arg(rust_triple);
    }

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("running cargo build -p dream-runner: {e}"))?;
    if !status.success() {
        bail!(
            "failed to build dream-runner for {rust_triple} (exit {:?}). \
             Install the target with `rustup target add {rust_triple}` and ensure a linker \
             is available for cross-compilation",
            status.code()
        );
    }

    let bin_name = if rust_triple.contains("windows") {
        "dream-runner.exe"
    } else {
        "dream-runner"
    };
    let built = if rust_triple == host_triple {
        dream_root.join("target").join("release").join(bin_name)
    } else {
        dream_root
            .join("target")
            .join(rust_triple)
            .join("release")
            .join(bin_name)
    };
    if !built.is_file() {
        bail!("cargo reported success but {} is missing", built.display());
    }
    std::fs::copy(&built, dest)
        .with_context(|| format!("copying {} → {}", built.display(), dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dest, perms)?;
    }
    Ok(())
}

fn host_rustc_triple() -> Result<String> {
    // Prefer rustc's host so we match cargo's default target directory layout.
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .context("running rustc -vV")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(host) = line.strip_prefix("host: ") {
            return Ok(host.trim().to_string());
        }
    }
    bail!("could not parse host triple from rustc -vV");
}
