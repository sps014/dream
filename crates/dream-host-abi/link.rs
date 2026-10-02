fn configure_library(capability: &str) {
    let name = format!("dream_host_{capability}");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo:rustc-link-arg-cdylib=-Wl,-soname,lib{name}.so");
            println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,$ORIGIN");
        }
        Ok("macos") => {
            println!("cargo:rustc-link-arg-cdylib=-Wl,-install_name,@rpath/lib{name}.dylib");
            println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,@loader_path");
        }
        _ => {}
    }
}

fn link_capability(capability: &str) {
    let directory = std::env::var("DEP_DREAM_HOST_CORE_LIBRARY_DIR")
        .expect("Cargo must build the core host before its capabilities");
    println!("cargo:rustc-link-search=native={directory}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg={directory}/dream_host_core.dll.lib");
    } else {
        println!("cargo:rustc-link-lib=dylib=dream_host_core");
    }
    println!("cargo:rerun-if-changed=../dream-host-abi/link.rs");
    configure_library(capability);
}
