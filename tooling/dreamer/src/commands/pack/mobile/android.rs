use super::{Options, Slice, bridge, command};
use anyhow::{Context, Result, bail};
use dream_abi::exports::ExportFunction;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;

fn ndk_clang(options: &Options) -> Result<PathBuf> {
    let root = options
        .ndk
        .clone()
        .or_else(|| std::env::var_os("ANDROID_NDK_HOME").map(PathBuf::from))
        .context("Android packaging requires --ndk PATH or ANDROID_NDK_HOME")?;
    let host = match std::env::consts::OS {
        "macos" => "darwin-x86_64",
        "linux" => "linux-x86_64",
        "windows" => "windows-x86_64",
        other => bail!("NDK has no toolchain for {other}"),
    };
    let clang = root
        .join("toolchains/llvm/prebuilt")
        .join(host)
        .join(if cfg!(windows) {
            "bin/clang.exe"
        } else {
            "bin/clang"
        });
    if !clang.is_file() {
        bail!("missing NDK clang {}", clang.display());
    }
    Ok(clang)
}

pub(super) fn pack(
    name: &str,
    slices: &BTreeMap<String, Slice>,
    functions: &[ExportFunction],
    options: &Options,
    writer: &super::super::bundle::BundleWriter,
) -> Result<()> {
    let stage = writer.root();
    if options.android_api < 21 {
        bail!("64-bit Android requires API 21 or newer");
    }
    let package = options
        .android_package
        .clone()
        .unwrap_or_else(|| format!("dream.generated.{name}"));
    bridge::java_package(&package)?;
    let clang = ndk_clang(options)?;
    let (java, c) = bridge::jni(name, &package, functions);
    let source = stage.join("DreamLibrary.java");
    std::fs::write(&source, &java)?;
    let classes = stage.join("classes");
    std::fs::create_dir_all(&classes)?;
    command(
        Command::new("javac")
            .args(["--release", "8", "-d"])
            .arg(&classes)
            .arg(&source),
        "compiling Java bridge (JDK 9+ required)",
    )?;
    let class_name = format!("{}/DreamLibrary.class", package.replace('.', "/"));
    let classes_jar = stage.join("classes.jar");
    write_zip(
        &classes_jar,
        &[(
            class_name,
            std::fs::read(
                classes
                    .join(package.replace('.', "/"))
                    .join("DreamLibrary.class"),
            )?,
        )],
    )?;
    let mut files = vec![
        ("AndroidManifest.xml".into(), format!("<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\" package=\"{package}\"><uses-sdk android:minSdkVersion=\"{}\" /></manifest>\n", options.android_api).into_bytes()),
        ("classes.jar".into(), std::fs::read(classes_jar)?),
        ("consumer-rules.pro".into(), format!("-keep class {package}.DreamLibrary {{ *; }}\n").into_bytes()),
        ("sources/DreamLibrary.java".into(), java.into_bytes()),
        ("sources/bridge.c".into(), c.as_bytes().to_vec()),
        ("sources/dream_library.h".into(), std::fs::read(&slices.values().next().context("no Android slices")?.header)?),
    ];
    for (triple, slice) in slices {
        let abi = if triple.starts_with("aarch64") {
            "arm64-v8a"
        } else {
            "x86_64"
        };
        let directory = stage.join(abi);
        std::fs::create_dir_all(&directory)?;
        std::fs::copy(&slice.header, directory.join("dream_library.h"))?;
        let library_name = slice.library.file_name().context("library filename")?;
        std::fs::copy(&slice.library, directory.join(library_name))?;
        let shim = directory.join("bridge.c");
        std::fs::write(&shim, &c)?;
        let product = directory.join(format!("lib{name}_jni.so"));
        command(
            Command::new(&clang)
                .arg(format!("--target={triple}{}", options.android_api))
                .args([
                    "-shared",
                    "-fPIC",
                    "-Wl,--no-undefined",
                    "-Wl,-z,max-page-size=16384",
                ])
                .arg(format!("-Wl,-soname,lib{name}_jni.so"))
                .arg(&shim)
                .arg("-I")
                .arg(&directory)
                .arg("-L")
                .arg(&directory)
                .arg(format!("-l:{}", library_name.to_string_lossy()))
                .arg("-o")
                .arg(&product),
            "linking Android JNI bridge",
        )?;
        files.push((
            format!("jni/{abi}/lib{name}_jni.so"),
            std::fs::read(product)?,
        ));
        files.push((
            format!("jni/{abi}/{}", library_name.to_string_lossy()),
            std::fs::read(&slice.library)?,
        ));
    }
    let staged = stage.join(format!("{name}.aar"));
    write_zip(&staged, &files)?;
    for destination in writer.publish(&[format!("{name}.aar").into()])? {
        println!("packed {}", destination.display());
    }
    Ok(())
}

pub(super) fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path)?);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let mut files = files.iter().collect::<Vec<_>>();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, bytes) in files {
        writer.start_file(name, options)?;
        writer.write_all(bytes)?;
    }
    writer.finish()?;
    Ok(())
}
