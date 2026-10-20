//! Builds slang-rhi plus Dream's C++ glue from pinned, hash-verified sources (pins.toml) with
//! CMake, links the static archives into the capability library, and stages Dawn's shared
//! library next to it. Nothing reaches the network once the GPU cache holds the archives.

include!("../dream-host-abi/link.rs");

#[path = "build/cmake_config.rs"]
mod cmake_config;
#[path = "build/fetch.rs"]
mod fetch;

use fetch::Pin;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    for path in [
        "build.rs",
        "build",
        "native",
        "patches",
        "pins.toml",
        "../dream-mir/src/runtime/c/core/gpu_bundle.c",
        "../dream-mir/src/runtime/c/include/dream_gpu_bundle.h",
        "../dream-mir/src/runtime/c/include/dream_gpu_bindings.h",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=DREAM_GPU_CACHE");
    println!("cargo:rerun-if-env-changed=DREAM_SLANG");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let platform = format!(
        "{}-{}",
        match os.as_str() {
            "macos" => "macos",
            "linux" => "linux",
            "windows" => "windows",
            other => panic!("dream-host-gpu does not support {other}"),
        },
        if arch == "aarch64" {
            "aarch64"
        } else {
            "x86_64"
        }
    );
    let pins: toml::Table = std::fs::read_to_string(manifest.join("pins.toml"))
        .expect("reading pins.toml")
        .parse()
        .expect("parsing pins.toml");
    let table = |key: &str| -> &toml::Table {
        pins.get(key)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("pins.toml has no [{key}]"))
    };
    let archive_pin = |key: &str| {
        let archives = table(key)
            .get("archives")
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("pins.toml: [{key}] has no archives"));
        let entry = archives
            .get(&platform)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("pins.toml: [{key}] has no archive for {platform}"));
        Pin::from_table(entry, key)
    };

    let cache = cache_dir();
    // Host tests look for the probe's Vulkan loader and MoltenVK here on macOS.
    println!("cargo:rustc-env=DREAM_GPU_CACHE_DIR={}", cache.display());
    let slang_version = table("slang")["version"].as_str().unwrap();
    let slang_include = slang_headers(&cache, slang_version, &platform, archive_pin("slang"));

    let rhi = table("slang-rhi");
    let rhi_commit = rhi["commit"].as_str().unwrap();
    let rhi_archive = fetch::download(&cache, &Pin::from_table(rhi, "slang-rhi"), None);
    let pristine = fetch::extract(
        &rhi_archive,
        &cache.join(format!("slang-rhi-{}-pristine", &rhi_commit[..12])),
        |_| true,
    );
    let patches: Vec<PathBuf> = rhi["patches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| manifest.join("patches").join(p.as_str().unwrap()))
        .collect();
    let patch_digest = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        for patch in &patches {
            hasher.update(std::fs::read(patch).expect("reading a slang-rhi patch"));
        }
        format!("{:x}", hasher.finalize())
    };
    let rhi_source = fetch::patched(
        &fetch::single_child(&pristine),
        &cache.join(format!(
            "slang-rhi-{}-{}",
            &rhi_commit[..12],
            &patch_digest[..12]
        )),
        &patches,
    );

    let dawn_pin = archive_pin("dawn");
    let dawn_version = table("dawn")["version"].as_str().unwrap();
    let dawn_archive = fetch::download(&cache, &dawn_pin, None);
    let vulkan_pin = Pin::from_table(table("vulkan-headers"), "vulkan-headers");
    let vulkan_archive = fetch::download(&cache, &vulkan_pin, None);

    let mut config = cmake::Config::new(manifest.join("native"));
    config
        .out_dir(cmake_config::output_dir())
        .profile("Release")
        .define("DREAM_SLANG_INCLUDE_DIR", &slang_include)
        .define("DREAM_SLANG_RHI_DIR", &rhi_source)
        .define("FETCHCONTENT_FULLY_DISCONNECTED", "OFF")
        .define("SLANG_RHI_DAWN_URL", file_url(&dawn_archive))
        .define(
            "SLANG_RHI_DAWN_URL_HASH",
            format!("SHA256={}", dawn_pin.sha256),
        )
        .define("SLANG_RHI_VULKAN_HEADERS_URL", file_url(&vulkan_archive))
        .define(
            "SLANG_RHI_VULKAN_HEADERS_URL_HASH",
            format!("SHA256={}", vulkan_pin.sha256),
        );
    if os == "macos" {
        let metal_pin = Pin::from_table(table("metal-cpp"), "metal-cpp");
        let metal_archive = fetch::download(&cache, &metal_pin, None);
        config
            .define("SLANG_RHI_METAL_CPP_URL", file_url(&metal_archive))
            .define(
                "SLANG_RHI_METAL_CPP_URL_HASH",
                format!("SHA256={}", metal_pin.sha256),
            );
    }
    if os == "windows" {
        let winpix_pin = Pin::from_table(table("winpix"), "winpix");
        let winpix_archive = fetch::download(&cache, &winpix_pin, Some("WinPixEventRuntime.zip"));
        config
            .define("SLANG_RHI_WINPIX_URL", file_url(&winpix_archive))
            .define(
                "SLANG_RHI_WINPIX_URL_HASH",
                format!("SHA256={}", winpix_pin.sha256),
            );
    }
    let installed = config.build();

    let lib = installed.join("lib");
    println!("cargo:rustc-link-search=native={}", lib.display());
    let archives = std::fs::read_to_string(lib.join("archives.txt")).expect("reading archives.txt");
    for archive in archives.lines().filter(|l| !l.is_empty()) {
        println!("cargo:rustc-link-lib=static={archive}");
    }
    match os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=dylib=objc");
            for framework in ["Foundation", "QuartzCore", "Metal", "AppKit"] {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
        }
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=pthread");
        }
        "windows" => {
            for system in [
                "d3d12", "d3d11", "dxgi", "dxguid", "user32", "advapi32", "psapi",
            ] {
                println!("cargo:rustc-link-lib=dylib={system}");
            }
        }
        _ => {}
    }

    let dawn = fetch::extract(
        &dawn_archive,
        &cache.join(format!("dawn-{dawn_version}-{platform}")),
        |path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            dawn_library_names().contains(&name)
        },
    );
    stage_dawn(&dawn);

    link_capability("gpu");
}

/// `DREAM_GPU_CACHE`, else `<target>/gpu-cache` next to the profile directories.
fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DREAM_GPU_CACHE") {
        return PathBuf::from(dir);
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    out.ancestors()
        .nth(4)
        .expect("Cargo target directory")
        .join("gpu-cache")
}

/// The Slang headers: from the `DREAM_SLANG` toolchain root when it has them, else from the
/// pinned release archive. Only `include/` is ever extracted.
fn slang_headers(cache: &Path, version: &str, platform: &str, pin: Pin) -> PathBuf {
    if let Some(root) = std::env::var_os("DREAM_SLANG") {
        let root = PathBuf::from(root);
        for candidate in [root.join("include"), root.join("../include")] {
            if candidate.join("slang.h").is_file()
                && std::fs::read_to_string(candidate.join("slang-tag-version.h"))
                    .is_ok_and(|text| text.contains(version))
            {
                return candidate;
            }
        }
    }
    let archive = fetch::download(cache, &pin, None);
    let extracted = fetch::extract(
        &archive,
        &cache.join(format!("slang-{version}-{platform}-include")),
        |path| path.components().any(|c| c.as_os_str() == "include"),
    );
    walk_for(&extracted, "slang.h")
        .and_then(|header| header.parent().map(Path::to_path_buf))
        .expect("the Slang archive has no include/slang.h")
}

fn walk_for(root: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path);
            }
        }
    }
    None
}

fn dawn_library_names() -> &'static [&'static str] {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => &["dawn.dll"],
        Ok("macos") => &["libdawn.dylib"],
        _ => &["libdawn.so"],
    }
}

/// slang-rhi loads Dawn from the directory of the library that links it, so Dawn ships next to
/// libdream_host_gpu in the profile directory (and `deps/`, where tests link from).
fn stage_dawn(extracted: &Path) {
    let name = dawn_library_names()[0];
    let source = walk_for(extracted, name).expect("the Dawn archive has no shared library");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    for dir in [profile.to_path_buf(), profile.join("deps")] {
        std::fs::create_dir_all(&dir).ok();
        let dest = dir.join(name);
        let stale = std::fs::metadata(&dest)
            .map(|m| m.len() != std::fs::metadata(&source).map(|s| s.len()).unwrap_or(0))
            .unwrap_or(true);
        if stale {
            std::fs::copy(&source, &dest).expect("staging Dawn next to the GPU host");
        }
    }
}

// CMake's verify step strips a literal `file://`, which turns `file:///D:/x` into `/D:/x`;
// ExternalProject accepts a bare local path, so drive-letter paths are passed without a scheme.
fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        path
    }
}
