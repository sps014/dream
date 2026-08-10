//! Copies pack-time embeds into `OUT_DIR` for `include_bytes!`.
//!
//! - `DREAM_EMBEDDED_WASM` → `embedded.wasm` (required for a real pack)
//! - `DREAM_EMBEDDED_ICON` → `embedded_icon.png` (optional app window icon)

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    let wasm_dest = out_dir.join("embedded.wasm");
    println!("cargo:rerun-if-env-changed=DREAM_EMBEDDED_WASM");
    if let Ok(src) = env::var("DREAM_EMBEDDED_WASM") {
        println!("cargo:rerun-if-changed={src}");
        fs::copy(&src, &wasm_dest).unwrap_or_else(|e| {
            panic!(
                "dream-runner: failed to copy DREAM_EMBEDDED_WASM ({src}) to {}: {e}",
                wasm_dest.display()
            )
        });
    } else if !wasm_dest.exists() {
        // Placeholder so the crate still compiles without an embed (cargo check / default builds).
        fs::write(&wasm_dest, b"\0asm\x01\x00\x00\x00").expect("write placeholder wasm");
    }

    let icon_dest = out_dir.join("embedded_icon.png");
    println!("cargo:rerun-if-env-changed=DREAM_EMBEDDED_ICON");
    if let Ok(src) = env::var("DREAM_EMBEDDED_ICON") {
        println!("cargo:rerun-if-changed={src}");
        fs::copy(&src, &icon_dest).unwrap_or_else(|e| {
            panic!(
                "dream-runner: failed to copy DREAM_EMBEDDED_ICON ({src}) to {}: {e}",
                icon_dest.display()
            )
        });
    println!("cargo:rerun-if-env-changed=DREAM_C_LIBS");
    if let Ok(libs) = env::var("DREAM_C_LIBS") {
        for lib in libs.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            // Auto-link `@c("lib", …)` libraries into the packed host (no CLI flags).
            println!("cargo:rustc-link-lib={lib}");
        }
    }
}
