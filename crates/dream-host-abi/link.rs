include!("profile.rs");

fn core_host_library_name() -> &'static str {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => "dream_host_core.dll",
        Ok("macos" | "ios") => "libdream_host_core.dylib",
        _ => "libdream_host_core.so",
    }
}

/// Directory containing the core host library. Cargo 1.99 keeps every cdylib in `<profile>/deps`.
/// Cargo 1.100 keeps a dependency cdylib in `<profile>/build/dream-host-core/<hash>/out`.
fn core_host_dir() -> std::path::PathBuf {
    let profile = cargo_profile_dir();
    let file = core_host_library_name();
    let shared = profile.join("deps");
    if shared.join(file).is_file() {
        return shared;
    }
    let units = profile.join("build").join("dream-host-core");
    let mut found = std::fs::read_dir(&units)
        .unwrap_or_else(|_| panic!("core host build directory {}", units.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("out"))
        .filter(|dir| dir.join(file).is_file())
        .collect::<Vec<_>>();
    found.sort_by_key(|dir| {
        std::fs::metadata(dir.join(file))
            .and_then(|meta| meta.modified())
            .ok()
    });
    found
        .pop()
        .unwrap_or_else(|| panic!("core host library {file} under {}", units.display()))
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
