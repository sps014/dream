//! Bakes `[package.metadata.dream] stack-size` from Cargo.toml into
//! `DREAM_DEFAULT_STACK_SIZE` for the native runtime (`execution::host::stack_size`).

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=DREAM_DEFAULT_STACK_SIZE");

    // Allow CI/local overrides without editing Cargo.toml.
    if let Ok(v) = std::env::var("DREAM_DEFAULT_STACK_SIZE") {
        if !v.trim().is_empty() {
            println!("cargo:rustc-env=DREAM_DEFAULT_STACK_SIZE={}", v.trim());
            return;
        }
    }

    let manifest = std::fs::read_to_string("Cargo.toml").unwrap_or_default();
    if let Some(size) = metadata_stack_size(&manifest) {
        println!("cargo:rustc-env=DREAM_DEFAULT_STACK_SIZE={}", size);
    }
}

/// Reads `stack-size = "..."` under `[package.metadata.dream]` (line-oriented; no toml crate).
fn metadata_stack_size(manifest: &str) -> Option<String> {
    let mut in_section = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[package.metadata.dream]";
            continue;
        }
        if !in_section || t.is_empty() || t.starts_with('#') {
            continue;
        }
        let rest = t.strip_prefix("stack-size")?;
        let rest = rest.trim().strip_prefix('=')?.trim();
        let quoted = rest.strip_prefix('"')?.strip_suffix('"')?;
        if !quoted.is_empty() {
            return Some(quoted.to_string());
        }
    }
    None
}
