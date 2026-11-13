include!("profile.rs");

/// Directory that will contain the core host library when the linker runs.
///
/// Cargo 1.99 keeps every cdylib in `<profile>/deps`. Cargo 1.100 keeps a dependency cdylib in
/// `<profile>/build/dream-host-core/<hash>/out`, and it runs this script while that library is
/// still being linked, so the file itself may not exist yet. The library compile unit is the
/// newest package directory that is neither the build-script binary nor its run directory.
fn core_host_dir() -> std::path::PathBuf {
    let profile = cargo_profile_dir();
    let units = profile.join("build").join("dream-host-core");
    if !units.is_dir() {
        return profile.join("deps");
    }
    let mut compiled = std::fs::read_dir(&units)
        .unwrap_or_else(|_| panic!("core host build directory {}", units.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir() && !dir.join("run").is_dir() && !is_build_script_bin(dir))
        .collect::<Vec<_>>();
    compiled.sort_by_key(|dir| dir.metadata().and_then(|meta| meta.modified()).ok());
    compiled
        .pop()
        .map(|dir| dir.join("out"))
        .unwrap_or_else(|| panic!("core host compile unit under {}", units.display()))
}

fn is_build_script_bin(unit: &std::path::Path) -> bool {
    let out = unit.join("out");
    out.join("build_script_build").exists() || out.join("build_script_build.exe").exists()
}

fn configure_library(capability: &str) {
    let name = format!("dream_host_{capability}");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo:rustc-link-arg-cdylib=-Wl,-soname,lib{name}.so");
            println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,$ORIGIN");
        }
        // The APK's library directory is already on the loader path. Android 15 devices may use
        // 16 KB pages, which reject 4 KB-aligned segments.
        Ok("android") => {
            println!("cargo:rustc-link-arg-cdylib=-Wl,-soname,lib{name}.so");
            println!("cargo:rustc-link-arg-cdylib=-Wl,-z,max-page-size=16384");
        }
        Ok("macos" | "ios") => {
            println!("cargo:rustc-link-arg-cdylib=-Wl,-install_name,@rpath/lib{name}.dylib");
            println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,@loader_path");
        }
        _ => {}
    }
}

fn link_capability(capability: &str) {
    let directory = core_host_dir();
    // A cdylib loaded from its build-unit directory does not sit next to core. The uplifted copy
    // in the profile directory still resolves through the loader path set below.
    if directory.file_name().and_then(|name| name.to_str()) == Some("out") {
        let hash = directory
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .expect("core host unit hash");
        let relative = format!("../../../dream-host-core/{hash}/out");
        match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("linux" | "android") => {
                println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,$ORIGIN/{relative}")
            }
            Ok("macos" | "ios") => {
                println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,@loader_path/{relative}")
            }
            _ => {}
        }
    }
    let directory = directory.display();
    println!("cargo:rustc-link-search=native={directory}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg={directory}/dream_host_core.dll.lib");
    } else {
        println!("cargo:rustc-link-lib=dylib=dream_host_core");
    }
    println!("cargo:rerun-if-changed=../dream-host-abi/link.rs");
    configure_library(capability);
}
