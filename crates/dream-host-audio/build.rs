include!("../dream-host-abi/link.rs");
fn main() {
    link_capability("audio");
    let mut build = cc::Build::new();
    build
        .file("native/output.c")
        .include("native/vendor")
        .warnings(false);
    // Miniaudio configures the iOS audio session through AVFoundation, which needs Objective-C.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        build.flag("-xobjective-c");
    }
    build.compile("dream_audio_output");
    println!("cargo:rerun-if-changed=native");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=m");
        }
        Ok("android") => {
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=m");
        }
        Ok("ios") => {
            println!("cargo:rustc-link-lib=framework=AVFoundation");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
        }
        Ok("macos") => {
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
        }
        _ => {}
    }
}
