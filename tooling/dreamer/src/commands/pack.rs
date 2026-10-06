//! `dreamer pack` produces a relocatable CLI executable and its selected host libraries.

pub mod bundle;
pub mod mobile;
mod runtime;

use crate::app_icon;
use crate::compile_flags::CompileFlags;
use crate::manifest::PackageType;
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Supported pack triples (Dream name → rustc target). Cross linking uses the compiler toolchain.
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
    mut flags: CompileFlags,
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
    flags.relocatable = true;
    let pkg_name = pkg.name.clone();
    for (dream_triple, rust_triple) in &triples {
        let spec = dream_abi::target::TargetSpec::parse(rust_triple).map_err(anyhow::Error::msg)?;
        let entry = workspace.compile_root_path()?;
        let stem = entry.file_stem().context("entry stem")?;
        let build_dir = workspace
            .root
            .join("target")
            .join(rust_triple)
            .join(flags.native_artifact_subdir());
        std::fs::create_dir_all(&build_dir)?;
        let ir = build_dir.join(stem).with_extension("ll");
        super::build::compile_target(
            &workspace,
            &flags,
            Some(crate::manifest::RunTarget::Native),
            Some((rust_triple, &ir)),
        )?;
        let bin_path = ir.with_extension("bin");
        if !bin_path.is_file() {
            bail!(
                "expected native binary at {} after build",
                bin_path.display()
            );
        }
        let pack_dir = workspace.root.join("target/pack").join(dream_triple);
        let writer = bundle::BundleWriter::new(&pack_dir)?;
        let mut products = Vec::new();
        runtime::copy(&bin_path, writer.root(), &spec)?;
        for capability in dream_abi::host_capability::HostCapability::ALL {
            let name = capability.library_name(&spec);
            if writer.root().join(&name).is_file() {
                products.push(PathBuf::from(name));
            }
        }
        let out_name = if dream_triple.starts_with("windows-") {
            format!("{pkg_name}-{dream_triple}.exe")
        } else {
            format!("{pkg_name}-{dream_triple}")
        };
        let dest = writer.path(&out_name)?;
        products.push(PathBuf::from(&out_name));
        std::fs::copy(&bin_path, &dest)
            .with_context(|| format!("copy {} → {}", bin_path.display(), dest.display()))?;
        app_icon::make_executable(&dest)?;
        for product in writer.publish_native(&products, &spec)? {
            println!("packed {}", product.display());
        }
    }
    Ok(())
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
        if args.len() != 1 {
            bail!("pack all cannot be combined with individual targets");
        }
        return Ok(PACK_TRIPLES
            .iter()
            .map(|(d, r)| (d.to_string(), r.to_string()))
            .collect());
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
        if !out.iter().any(|(name, _)| name == a) {
            out.push((a.clone(), rust));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expands_cross_targets_and_deduplicates_explicit_targets() {
        assert_eq!(
            resolve_pack_targets(&["all".into()]).unwrap().len(),
            PACK_TRIPLES.len()
        );
        assert_eq!(
            resolve_pack_targets(&[
                "linux-x64".into(),
                "linux-x64".into(),
                "windows-arm64".into()
            ])
            .unwrap()
            .len(),
            2
        );
        assert!(resolve_pack_targets(&["all".into(), "linux-x64".into()]).is_err());
        assert!(resolve_pack_targets(&["unknown".into()]).is_err());
    }
}
