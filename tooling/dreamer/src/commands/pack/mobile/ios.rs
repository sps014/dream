use super::{bridge, command, Slice};
use anyhow::{bail, Context, Result};
use dream_abi::exports::ExportFunction;
use std::collections::BTreeMap;
use std::process::Command;

pub(super) fn pack(
    name: &str,
    slices: &BTreeMap<String, Slice>,
    functions: &[ExportFunction],
    writer: &super::super::bundle::BundleWriter,
) -> Result<()> {
    let stage = writer.root();
    if !cfg!(target_os = "macos") {
        bail!("iOS packaging requires macOS and Xcode");
    }
    let class = format!("Dream_{name}");
    let (header, source) = bridge::objc(name, functions);
    let mut create = Command::new("xcodebuild");
    create.arg("-create-xcframework");
    for (triple, slice) in slices {
        let directory = stage.join(triple);
        let headers = directory.join("Headers");
        std::fs::create_dir_all(&headers)?;
        std::fs::copy(&slice.header, headers.join("dream_library.h"))?;
        std::fs::write(headers.join(format!("{class}.h")), &header)?;
        std::fs::write(
            headers.join("module.modulemap"),
            format!("module {class} {{ umbrella header \"{class}.h\" export * }}\n"),
        )?;
        let shim = directory.join("bridge.m");
        std::fs::write(&shim, &source)?;
        let sdk = if triple.ends_with("simulator") {
            "iphonesimulator"
        } else {
            "iphoneos"
        };
        let query = Command::new("xcrun")
            .args(["--sdk", sdk, "--show-sdk-path"])
            .output()
            .context("finding Xcode iOS SDK")?;
        if !query.status.success() {
            bail!(
                "Xcode SDK {sdk} unavailable: install full Xcode and select it with xcode-select"
            );
        }
        let sdk_path = String::from_utf8(query.stdout)?.trim().to_string();
        let object = directory.join("bridge.o");
        command(
            Command::new("xcrun")
                .args([
                    "--sdk",
                    sdk,
                    "clang",
                    "-target",
                    &slice.target.llvm_triple(),
                    "-isysroot",
                    &sdk_path,
                    "-fobjc-arc",
                    "-c",
                ])
                .arg(&shim)
                .arg("-I")
                .arg(&headers)
                .arg("-o")
                .arg(&object),
            "compiling Objective-C bridge",
        )?;
        let library = directory.join(format!("lib{name}.a"));
        command(
            Command::new("xcrun")
                .args(["--sdk", sdk, "libtool", "-static", "-o"])
                .arg(&library)
                .arg(&slice.library)
                .arg(&object),
            "archiving iOS slice",
        )?;
        create
            .arg("-library")
            .arg(&library)
            .arg("-headers")
            .arg(&headers);
    }
    let product = stage.join(format!("{name}.xcframework"));
    command(create.arg("-output").arg(&product), "creating XCFramework")?;
    for destination in writer.publish(&[format!("{name}.xcframework").into()])? {
        println!("packed {}", destination.display());
    }
    Ok(())
}
