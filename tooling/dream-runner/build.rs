//! Copies pack-time embeds into `OUT_DIR` for `include_bytes!`.
//!
//! - `DREAM_EMBEDDED_WASM` → `embedded.wasm` (required for a real pack)
//! - `DREAM_EMBEDDED_ABI` → `embedded.abi.json` (`@c` / GPU metadata for the packed host)
//! - `DREAM_EMBEDDED_ICON` → `embedded_icon.png` (optional app window icon)
//! - `DREAM_C_LIBS` → `cargo:rustc-link-lib=…` for `@c` libraries (comma-separated)

use std::env;
use std::fs;
use std::path::PathBuf;

/// 1×1 transparent PNG (68 bytes) used when no `[package].icon` is packed.
const MIN_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

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

    let abi_dest = out_dir.join("embedded.abi.json");
    println!("cargo:rerun-if-env-changed=DREAM_EMBEDDED_ABI");
    if let Ok(src) = env::var("DREAM_EMBEDDED_ABI") {
        println!("cargo:rerun-if-changed={src}");
        fs::copy(&src, &abi_dest).unwrap_or_else(|e| {
            panic!(
                "dream-runner: failed to copy DREAM_EMBEDDED_ABI ({src}) to {}: {e}",
                abi_dest.display()
            )
        });
    } else if !abi_dest.exists() {
        fs::write(&abi_dest, b"{}").expect("write placeholder abi");
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
    } else if !icon_dest.exists() {
        // Tiny valid PNG so `include_bytes!` always succeeds when no package icon is set.
        fs::write(&icon_dest, MIN_PNG).expect("write placeholder icon");
    }

    println!("cargo:rerun-if-env-changed=DREAM_C_LIBS");
    if let Ok(libs) = env::var("DREAM_C_LIBS") {
        for lib in libs.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            // Auto-link `@c("lib", …)` libraries into the packed host (no CLI flags).
            println!("cargo:rustc-link-lib={lib}");
        }
    }
}
