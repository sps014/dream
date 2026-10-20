include!("../dream-host-abi/link.rs");
fn main() {
    link_capability("audio");
    cc::Build::new()
        .file("native/output.c")
        .include("native/vendor")
        .warnings(false)
        .compile("dream_audio_output");
    println!("cargo:rerun-if-changed=native");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=m");
        }
        Ok("macos") => {
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
        }
        _ => {}
    }
}
