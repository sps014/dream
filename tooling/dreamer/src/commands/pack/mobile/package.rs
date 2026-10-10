use super::{android, bridge, ios};
use anyhow::{Context, Result, bail};
use dream_abi::exports::ExportFunction;
use dream_abi::target::TargetSpec;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Options {
    pub platform: String,
    pub slices: Vec<String>,
    pub android_package: Option<String>,
    pub ndk: Option<PathBuf>,
    pub android_api: u32,
}

#[derive(Deserialize)]
pub(super) struct LibraryAbi {
    target_triple: String,
    export_functions: Vec<ExportFunction>,
    exports: Vec<String>,
}

pub(super) struct Slice {
    pub(super) library: PathBuf,
    pub(super) header: PathBuf,
    pub(super) abi: LibraryAbi,
    pub(super) target: TargetSpec,
}

pub fn run(start: &Path, package: Option<&str>, options: Options) -> Result<()> {
    let workspace = crate::workspace::Workspace::discover_package(start, package)?;
    let package = workspace.manifest.package()?;
    if package.package_type != crate::manifest::PackageType::Lib {
        bail!("mobile packaging requires a type = \"lib\" package");
    }
    let slices = read_slices(&options.platform, &options.slices)?;
    let name = package.name.replace('-', "_");
    if !identifier(&name) {
        bail!("mobile package name must be a C identifier (hyphens become underscores)");
    }
    let output = workspace.root.join("target/pack");
    std::fs::create_dir_all(&output)?;
    let writer = super::super::bundle::BundleWriter::new(&output)?;
    let functions = &slices
        .values()
        .next()
        .context("no mobile slices")?
        .abi
        .export_functions;
    match options.platform.as_str() {
        "ios" => ios::pack(&name, &slices, functions, &writer)?,
        "android" => android::pack(&name, &slices, functions, &options, &writer)?,
        _ => bail!("mobile platform must be ios or android"),
    }
    Ok(())
}

pub(super) fn read_slices(platform: &str, values: &[String]) -> Result<BTreeMap<String, Slice>> {
    let required = match platform {
        "ios" => ["arm64-apple-ios", "arm64-apple-ios-simulator"],
        "android" => ["aarch64-linux-android", "x86_64-linux-android"],
        _ => bail!("mobile platform must be ios or android"),
    };
    let mut slices = BTreeMap::new();
    for value in values {
        let (triple, path) = value
            .split_once('=')
            .context("--slice expects TRIPLE=LIBRARY")?;
        if !required.contains(&triple) {
            bail!("unsupported {platform} slice {triple}");
        }
        let library = Path::new(path)
            .canonicalize()
            .with_context(|| format!("reading slice {path}"))?;
        let extension = if platform == "ios" { "a" } else { "so" };
        if library.extension().and_then(|e| e.to_str()) != Some(extension) {
            bail!("{platform} slices require .{extension} libraries");
        }
        let stem = library
            .file_stem()
            .and_then(|s| s.to_str())
            .context("invalid library filename")?;
        if !identifier(stem) {
            bail!("slice filename must be a C identifier");
        }
        let abi_path = library.with_extension("abi.json");
        let abi: LibraryAbi = serde_json::from_slice(
            &std::fs::read(&abi_path).with_context(|| format!("reading {}", abi_path.display()))?,
        )?;
        let target = TargetSpec::parse(&abi.target_triple).map_err(anyhow::Error::msg)?;
        let selected = TargetSpec::parse(triple).map_err(anyhow::Error::msg)?;
        if target.triple != selected.triple {
            bail!("slice {triple} has ABI target {}", abi.target_triple);
        }
        if abi.exports.iter().any(|s| s == "main") {
            bail!("mobile library must not export main");
        }
        if abi.export_functions.is_empty() {
            bail!("mobile library has no @export functions");
        }
        if abi.exports
            != abi
                .export_functions
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
        {
            bail!("export signature inventory does not match exported symbols");
        }
        bridge::validate(&abi.export_functions)?;
        let header = library.with_extension("h");
        if !header.is_file() {
            bail!("missing generated header {}", header.display());
        }
        if let Some(first) = slices.values().next() {
            let first: &Slice = first;
            if first.abi.export_functions != abi.export_functions
                || std::fs::read(&first.header)? != std::fs::read(&header)?
            {
                bail!("mobile slices have different export signatures or headers");
            }
        }
        if slices
            .insert(
                triple.to_string(),
                Slice {
                    library,
                    header,
                    abi,
                    target,
                },
            )
            .is_some()
        {
            bail!("duplicate slice {triple}");
        }
    }
    for triple in required {
        if !slices.contains_key(triple) {
            bail!("missing --slice {triple}=LIBRARY");
        }
    }
    Ok(slices)
}

pub(super) fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) fn command(cmd: &mut Command, action: &str) -> Result<()> {
    let output = cmd.output().with_context(|| format!("{action}: {cmd:?}"))?;
    if !output.status.success() {
        bail!("{action}: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}
