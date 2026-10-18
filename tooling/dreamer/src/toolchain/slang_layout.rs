//! Builds `dream-slang-layout`, the Slang layout recorder the GPU shader generator runs next to
//! `slangc`, into an extracted Slang release. Slang publishes no such tool, and the recorder must
//! link the exact compiler it reflects, so it is compiled here from the sources the native GPU
//! host's replay library shares, against the release's own headers and compiler library. The
//! host C++ compiler is located the way Cargo build scripts locate it (the `cc` crate).

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const SOURCES: &[(&str, &str)] = &[
    (
        "recorder/recorder.cpp",
        include_str!("../../../../crates/dream-host-gpu/native/recorder/recorder.cpp"),
    ),
    (
        "replay/layout_blob.cpp",
        include_str!("../../../../crates/dream-host-gpu/native/replay/layout_blob.cpp"),
    ),
    (
        "replay/layout_blob.h",
        include_str!("../../../../crates/dream-host-gpu/native/replay/layout_blob.h"),
    ),
];

const STAMP: &str = "share/dream-slang-layout/source.sha256";

fn fingerprint() -> String {
    let mut hash = Sha256::new();
    hash.update(include_str!("slang_layout.rs"));
    for (path, source) in SOURCES {
        hash.update(path.as_bytes());
        hash.update(source.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

pub fn is_current(root: &Path) -> bool {
    fs::read_to_string(root.join(STAMP)).is_ok_and(|stamp| stamp == fingerprint())
}

/// The Rust target triple of this dreamer build, which is the host the tool runs on.
fn host_triple() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        (os, arch) => bail!("no C++ toolchain mapping for {os}-{arch}"),
    })
}

/// Compiles `<root>/bin/dream-slang-layout` against the Slang release extracted at `root`.
pub fn build(root: &Path) -> Result<()> {
    let source_dir = root.join("share").join("dream-slang-layout");
    for (path, text) in SOURCES {
        let dest = source_dir.join(path);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::write(&dest, text)?;
    }
    let triple = host_triple()?;
    let compiler = cc::Build::new()
        .cpp(true)
        .target(triple)
        .host(triple)
        .opt_level(2)
        .cargo_metadata(false)
        .cargo_warnings(false)
        .try_get_compiler()
        .context("finding a C++ compiler to build dream-slang-layout")?;
    let mut command = compiler.to_command();
    let sources = [
        source_dir.join("recorder/recorder.cpp"),
        source_dir.join("replay/layout_blob.cpp"),
    ];
    let objects = source_dir.join("obj");
    fs::create_dir_all(&objects)?;
    if compiler.is_like_msvc() {
        command
            .args(["/nologo", "/EHsc", "/std:c++20", "/O2"])
            .arg(format!("/I{}", source_dir.display()))
            .arg(format!("/I{}", root.join("include").display()))
            .args(&sources)
            .arg(format!("/Fo{}\\", objects.display()))
            .arg(format!(
                "/Fe{}",
                root.join("bin").join("dream-slang-layout.exe").display()
            ))
            .arg("/link")
            .arg(root.join("lib").join("slang-compiler.lib"));
    } else {
        let rpath = if cfg!(target_os = "macos") {
            "-Wl,-rpath,@loader_path/../lib"
        } else {
            "-Wl,-rpath,$ORIGIN/../lib"
        };
        command
            .args(["-std=c++20", "-O2"])
            .arg("-I")
            .arg(&source_dir)
            .arg("-I")
            .arg(root.join("include"))
            .args(&sources)
            .arg("-o")
            .arg(root.join("bin").join("dream-slang-layout"))
            .arg("-L")
            .arg(root.join("lib"))
            .arg("-lslang-compiler")
            .arg(rpath);
    }
    let output = command
        .output()
        .with_context(|| format!("running {}", compiler.path().display()))?;
    if !output.status.success() {
        bail!(
            "building dream-slang-layout failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = fs::remove_dir_all(&objects);
    fs::write(root.join(STAMP), fingerprint())?;
    Ok(())
}
