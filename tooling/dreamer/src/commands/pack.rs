//! `dreamer pack`: compile a bin package and copy the native `.bin`.

use crate::compile_flags::CompileFlags;
use crate::manifest::PackageType;
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Supported pack triples (Dream name → rustc target). Host-only until cross-cc is wired.
const PACK_TRIPLES: &[(&str, &str)] = &[
    ("linux-x64", "x86_64-unknown-linux-gnu"),
    ("linux-arm64", "aarch64-unknown-linux-gnu"),
    ("macos-x64", "x86_64-apple-darwin"),
    ("macos-arm64", "aarch64-apple-darwin"),
    ("windows-x64", "x86_64-pc-windows-msvc"),
    ("windows-arm64", "aarch64-pc-windows-msvc"),
];

pub fn run(
    start_dir: &Path,
    target_args: &[String],
    package: Option<&str>,
    flags: CompileFlags,
) -> Result<()> {
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
    super::build::compile_entry(&workspace, &flags, Some(crate::manifest::RunTarget::Native))?;

    let bin_path = artifact_native_bin(&workspace, &flags)?;
    if !bin_path.is_file() {
        bail!(
            "expected native binary at {} after build",
            bin_path.display()
        );
    }

    let pack_dir = workspace.root.join("target").join("pack");
    std::fs::create_dir_all(&pack_dir)
        .with_context(|| format!("creating {}", pack_dir.display()))?;

    let host_triple = host_rustc_triple()?;
    let pkg_name = pkg.name.clone();
    for (dream_triple, rust_triple) in &triples {
        if rust_triple.as_str() != host_triple {
            bail!(
                "cross-pack to {dream_triple} is not supported (native C pack is host-only; host is {host_triple})"
            );
        }
        let out_name = if dream_triple.starts_with("windows-") {
            format!("{pkg_name}-{dream_triple}.exe")
        } else {
            format!("{pkg_name}-{dream_triple}")
        };
        let dest = pack_dir.join(&out_name);
        std::fs::copy(&bin_path, &dest)
            .with_context(|| format!("copy {} → {}", bin_path.display(), dest.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&dest)?.permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&dest, p)?;
        }
        println!("packed {}", dest.display());
    }
    Ok(())
}

fn artifact_native_bin(workspace: &Workspace, flags: &CompileFlags) -> Result<PathBuf> {
    let entry = workspace.compile_root_path()?;
    let stem = entry
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("entry has no file stem"))?;
    Ok(workspace
        .root
        .join("target")
        .join(flags.native_artifact_subdir())
        .join(format!("{stem}.bin")))
}

fn resolve_pack_targets(args: &[String]) -> Result<Vec<(String, String)>> {
    if args.is_empty() {
        let host = host_pack_triple()?;
        let rust = PACK_TRIPLES
            .iter()
            .find(|(d, _)| *d == host)
            .map(|(_, r)| (*r).to_string())
            .ok_or_else(|| anyhow::anyhow!("internal: unknown host pack triple {host}"))?;
        return Ok(vec![(host, rust)]);
    }
    if args.iter().any(|a| a == "all") {
        bail!("pack 'all' requires cross-compilation; native C pack is host-only");
    }
    let mut out = Vec::new();
    for a in args {
        let rust = PACK_TRIPLES
            .iter()
            .find(|(d, _)| *d == a)
            .map(|(_, r)| (*r).to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown pack target '{}'; expected one of {} or host default",
                    a,
                    PACK_TRIPLES
                        .iter()
                        .map(|(d, _)| *d)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        out.push((a.clone(), rust));
    }
    Ok(out)
}

fn host_pack_triple() -> Result<String> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64".into(),
        ("linux", "aarch64") => "linux-arm64".into(),
        ("macos", "x86_64") => "macos-x64".into(),
        ("macos", "aarch64") => "macos-arm64".into(),
        ("windows", "x86_64") => "windows-x64".into(),
        ("windows", "aarch64") => "windows-arm64".into(),
        (os, arch) => bail!("unsupported host OS/arch for pack: {os}/{arch}"),
    })
}

fn host_rustc_triple() -> Result<String> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu".into(),
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu".into(),
        ("macos", "x86_64") => "x86_64-apple-darwin".into(),
        ("macos", "aarch64") => "aarch64-apple-darwin".into(),
        ("windows", "x86_64") => "x86_64-pc-windows-msvc".into(),
        ("windows", "aarch64") => "aarch64-pc-windows-msvc".into(),
        (os, arch) => bail!("unsupported host OS/arch for pack: {os}/{arch}"),
    })
}
